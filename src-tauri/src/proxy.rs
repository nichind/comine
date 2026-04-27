use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Proxy cache
// ---------------------------------------------------------------------------

struct ProxyCache {
    result: Option<ResolvedProxy>,
    last_check: Option<Instant>,
}

impl ProxyCache {
    const fn new() -> Self {
        Self {
            result: None,
            last_check: None,
        }
    }
}

static PROXY_CACHE: Mutex<ProxyCache> = Mutex::new(ProxyCache::new());
const PROXY_CACHE_TTL: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

/// Per-proxy health state tracked for the current session.
struct ProxyHealthState {
    consecutive_failures: u32,
    broken: bool,
}

impl ProxyHealthState {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            broken: false,
        }
    }
}

/// The circuit breaker trips after this many consecutive failures for a proxy URL.
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

static PROXY_HEALTH: Mutex<Option<HashMap<String, ProxyHealthState>>> = Mutex::new(None);

fn with_health<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, ProxyHealthState>) -> R,
{
    let mut guard = PROXY_HEALTH.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    f(map)
}

/// Record a failure for a proxy URL. Returns `true` if this call tripped the
/// circuit breaker (i.e. the proxy just transitioned to "broken").
fn record_failure_inner(url: &str) -> bool {
    with_health(|map| {
        let entry = map.entry(url.to_string()).or_insert_with(ProxyHealthState::new);
        if entry.broken {
            return false; // already broken, nothing new
        }
        entry.consecutive_failures += 1;
        if entry.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
            entry.broken = true;
            warn!(
                "proxy: Circuit breaker tripped for {} after {} consecutive failures — \
                 switching to direct for this session",
                url, entry.consecutive_failures
            );
            true
        } else {
            debug!(
                "proxy: Failure {}/{} for {}",
                entry.consecutive_failures, CIRCUIT_BREAKER_THRESHOLD, url
            );
            false
        }
    })
}

fn is_broken(url: &str) -> bool {
    with_health(|map| map.get(url).is_some_and(|s| s.broken))
}

/// Report a successful request through the given proxy URL.
/// Resets the consecutive failure counter (but does not un-break an already-broken proxy).
pub fn record_proxy_success(url: &str) {
    with_health(|map| {
        if let Some(entry) = map.get_mut(url) {
            if !entry.broken {
                entry.consecutive_failures = 0;
            }
        }
    });
}

/// Report a failed request through the given proxy URL.
/// After `CIRCUIT_BREAKER_THRESHOLD` consecutive failures the proxy is marked
/// broken and the cache is invalidated so the next resolution skips it.
pub fn record_proxy_failure(url: &str) {
    let tripped = record_failure_inner(url);
    if tripped {
        invalidate_proxy_cache();
    }
}

/// Reset all proxy health state (useful for UI "reset proxy" action).
pub fn reset_proxy_health() {
    with_health(|map| map.clear());
    info!("proxy: Health state reset");
}

// ---------------------------------------------------------------------------
// Cache invalidation
// ---------------------------------------------------------------------------

/// Clear the proxy detection cache so the next call to `detect_system_proxy()`
/// performs a fresh detection pass.
pub fn invalidate_proxy_cache() {
    if let Ok(mut cache) = PROXY_CACHE.lock() {
        cache.result = None;
        cache.last_check = None;
        info!("proxy: Cache invalidated");
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub mode: String,
    pub custom_url: String,
    pub retry_without_proxy: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: "system".to_string(),
            custom_url: String::new(),
            retry_without_proxy: true,
        }
    }
}

#[derive(serde::Serialize, Clone, Debug)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedProxy {
    pub url: String,
    pub source: String,
    pub description: String,
}

impl Default for ResolvedProxy {
    fn default() -> Self {
        Self {
            url: String::new(),
            source: "none".to_string(),
            description: "No proxy".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_proxy_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("Proxy URL is empty".to_string());
    }

    let valid_schemes = ["http://", "https://", "socks4://", "socks5://", "socks://"];
    if !valid_schemes.iter().any(|s| url.starts_with(s)) {
        return Err(format!(
            "Invalid proxy scheme. Must start with one of: {}",
            valid_schemes.join(", ")
        ));
    }

    let url_obj = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    if url_obj.host_str().is_none() {
        return Err("Proxy URL must have a host".to_string());
    }

    if url_obj.port().is_none() {
        warn!("Proxy URL has no port specified, using default port");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

pub fn resolve_proxy(config: &ProxyConfig) -> ResolvedProxy {
    match config.mode.as_str() {
        "none" => {
            info!("Proxy mode: none - no proxy will be used");
            ResolvedProxy::default()
        }
        "custom" => {
            if config.custom_url.is_empty() {
                warn!("Custom proxy mode but URL is empty, falling back to system");
                if config.retry_without_proxy {
                    detect_system_proxy()
                } else {
                    ResolvedProxy::default()
                }
            } else if let Err(e) = validate_proxy_url(&config.custom_url) {
                warn!("Invalid custom proxy URL: {}, falling back", e);
                if config.retry_without_proxy {
                    detect_system_proxy()
                } else {
                    ResolvedProxy::default()
                }
            } else {
                info!("Using custom proxy: {}", config.custom_url);
                ResolvedProxy {
                    url: config.custom_url.clone(),
                    source: "custom".to_string(),
                    description: format!("Custom proxy: {}", config.custom_url),
                }
            }
        }
        _ => {
            info!("Proxy mode: system - detecting system proxy");
            detect_system_proxy()
        }
    }
}

pub fn detect_system_proxy() -> ResolvedProxy {
    if let Ok(cache) = PROXY_CACHE.lock() {
        if let (Some(ref result), Some(last_check)) = (&cache.result, cache.last_check) {
            if last_check.elapsed() < PROXY_CACHE_TTL {
                debug!("Using cached proxy result");
                return result.clone();
            }
        }
    }

    let result = detect_system_proxy_uncached();

    if let Ok(mut cache) = PROXY_CACHE.lock() {
        cache.result = Some(result.clone());
        cache.last_check = Some(Instant::now());
    }

    result
}

fn detect_system_proxy_uncached() -> ResolvedProxy {
    if let Some(proxy) = detect_env_proxy() {
        return proxy;
    }

    #[cfg(target_os = "windows")]
    if let Some(proxy) = detect_windows_proxy() {
        return proxy;
    }

    #[cfg(target_os = "macos")]
    if let Some(proxy) = detect_macos_proxy() {
        return proxy;
    }

    #[cfg(target_os = "linux")]
    if let Some(proxy) = detect_linux_proxy() {
        return proxy;
    }

    // Port scanning is a last resort and generates false positives — log a warning.
    warn!(
        "proxy: No system proxy configured, falling back to port scanning \
         (may produce false positives)"
    );

    if let Some(proxy) = detect_common_proxy_ports() {
        return proxy;
    }

    info!("No system proxy detected");
    ResolvedProxy::default()
}

fn detect_env_proxy() -> Option<ResolvedProxy> {
    let proxy_vars = [
        ("HTTPS_PROXY", "https_proxy"),
        ("HTTP_PROXY", "http_proxy"),
        ("ALL_PROXY", "all_proxy"),
    ];

    for (upper, lower) in proxy_vars {
        for var in [upper, lower] {
            if let Ok(value) = std::env::var(var) {
                if !value.is_empty() {
                    info!("Found proxy in environment variable {}: {}", var, value);
                    return Some(ResolvedProxy {
                        url: value.clone(),
                        source: "system".to_string(),
                        description: format!("Environment variable: {}", var),
                    });
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn detect_windows_proxy() -> Option<ResolvedProxy> {
    let enable_output = crate::utils::new_std_command("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .output();

    if let Ok(output) = enable_output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("0x1") && !stdout.contains("REG_DWORD    1") {
            return None;
        }
    } else {
        return None;
    }

    let server_output = crate::utils::new_std_command("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
        ])
        .output();

    if let Ok(output) = server_output {
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.contains("ProxyServer") && line.contains("REG_SZ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(proxy_value) = parts.last() {
                    let proxy_value = proxy_value.trim();
                    if !proxy_value.is_empty() && proxy_value != "ProxyServer" {
                        let proxy_url = if proxy_value.contains('=') {
                            proxy_value
                                .split(';')
                                .find_map(|part| {
                                    let (protocol, server) = part.split_once('=')?;
                                    if protocol == "https" || protocol == "http" {
                                        Some(format!("http://{}", server))
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_else(|| {
                                    format!(
                                        "http://{}",
                                        proxy_value.split(';').next().unwrap_or("")
                                    )
                                })
                        } else if proxy_value.starts_with("http://")
                            || proxy_value.starts_with("socks")
                        {
                            proxy_value.to_string()
                        } else {
                            format!("http://{}", proxy_value)
                        };

                        info!("Found Windows system proxy: {}", proxy_url);
                        return Some(ResolvedProxy {
                            url: proxy_url.clone(),
                            source: "system".to_string(),
                            description: format!("Windows registry: {}", proxy_url),
                        });
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn detect_macos_proxy() -> Option<ResolvedProxy> {
    use std::process::Command;

    let output = Command::new("scutil").args(["--proxy"]).output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut http_enabled = false;
        let mut https_enabled = false;
        let mut socks_enabled = false;
        let mut http_host = String::new();
        let mut http_port = String::new();
        let mut https_host = String::new();
        let mut https_port = String::new();
        let mut socks_host = String::new();
        let mut socks_port = String::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                match parts[0] {
                    "HTTPEnable" => http_enabled = parts[1] == "1",
                    "HTTPProxy" => http_host = parts[1].to_string(),
                    "HTTPPort" => http_port = parts[1].to_string(),
                    "HTTPSEnable" => https_enabled = parts[1] == "1",
                    "HTTPSProxy" => https_host = parts[1].to_string(),
                    "HTTPSPort" => https_port = parts[1].to_string(),
                    "SOCKSEnable" => socks_enabled = parts[1] == "1",
                    "SOCKSProxy" => socks_host = parts[1].to_string(),
                    "SOCKSPort" => socks_port = parts[1].to_string(),
                    _ => {}
                }
            }
        }

        if https_enabled && !https_host.is_empty() && !https_port.is_empty() {
            let proxy_url = format!("http://{}:{}", https_host, https_port);
            info!("Found macOS HTTPS proxy: {}", proxy_url);
            return Some(ResolvedProxy {
                url: proxy_url.clone(),
                source: "system".to_string(),
                description: format!("macOS HTTPS proxy: {}", proxy_url),
            });
        }

        if http_enabled && !http_host.is_empty() && !http_port.is_empty() {
            let proxy_url = format!("http://{}:{}", http_host, http_port);
            info!("Found macOS HTTP proxy: {}", proxy_url);
            return Some(ResolvedProxy {
                url: proxy_url.clone(),
                source: "system".to_string(),
                description: format!("macOS HTTP proxy: {}", proxy_url),
            });
        }

        if socks_enabled && !socks_host.is_empty() && !socks_port.is_empty() {
            let proxy_url = format!("socks5://{}:{}", socks_host, socks_port);
            info!("Found macOS SOCKS proxy: {}", proxy_url);
            return Some(ResolvedProxy {
                url: proxy_url.clone(),
                source: "system".to_string(),
                description: format!("macOS SOCKS proxy: {}", proxy_url),
            });
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn detect_linux_proxy() -> Option<ResolvedProxy> {
    use std::process::Command;

    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
    {
        let mode = String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();

        if mode == "manual" {
            if let (Ok(host_output), Ok(port_output)) = (
                Command::new("gsettings")
                    .args(["get", "org.gnome.system.proxy.http", "host"])
                    .output(),
                Command::new("gsettings")
                    .args(["get", "org.gnome.system.proxy.http", "port"])
                    .output(),
            ) {
                let host = String::from_utf8_lossy(&host_output.stdout)
                    .trim()
                    .trim_matches('\'')
                    .to_string();
                let port = String::from_utf8_lossy(&port_output.stdout)
                    .trim()
                    .to_string();

                if !host.is_empty() && host != "''" && !port.is_empty() && port != "0" {
                    let proxy_url = format!("http://{}:{}", host, port);
                    info!("Found GNOME HTTP proxy: {}", proxy_url);
                    return Some(ResolvedProxy {
                        url: proxy_url.clone(),
                        source: "system".to_string(),
                        description: format!("GNOME HTTP proxy: {}", proxy_url),
                    });
                }
            }

            if let (Ok(host_output), Ok(port_output)) = (
                Command::new("gsettings")
                    .args(["get", "org.gnome.system.proxy.socks", "host"])
                    .output(),
                Command::new("gsettings")
                    .args(["get", "org.gnome.system.proxy.socks", "port"])
                    .output(),
            ) {
                let host = String::from_utf8_lossy(&host_output.stdout)
                    .trim()
                    .trim_matches('\'')
                    .to_string();
                let port = String::from_utf8_lossy(&port_output.stdout)
                    .trim()
                    .to_string();

                if !host.is_empty() && host != "''" && !port.is_empty() && port != "0" {
                    let proxy_url = format!("socks5://{}:{}", host, port);
                    info!("Found GNOME SOCKS proxy: {}", proxy_url);
                    return Some(ResolvedProxy {
                        url: proxy_url.clone(),
                        source: "system".to_string(),
                        description: format!("GNOME SOCKS proxy: {}", proxy_url),
                    });
                }
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let kde_config = std::path::Path::new(&home).join(".config/kioslaverc");
        if kde_config.exists() {
            if let Ok(content) = std::fs::read_to_string(&kde_config) {
                let mut proxy_type = 0;
                let mut http_proxy = String::new();

                for line in content.lines() {
                    if line.starts_with("ProxyType=") {
                        proxy_type = line.trim_start_matches("ProxyType=").parse().unwrap_or(0);
                    }
                    if line.starts_with("httpProxy=") {
                        http_proxy = line.trim_start_matches("httpProxy=").to_string();
                    }
                }

                if proxy_type == 1 && !http_proxy.is_empty() {
                    let proxy_url = if http_proxy.starts_with("http://") {
                        http_proxy.clone()
                    } else {
                        format!("http://{}", http_proxy)
                    };
                    info!("Found KDE proxy: {}", proxy_url);
                    return Some(ResolvedProxy {
                        url: proxy_url.clone(),
                        source: "system".to_string(),
                        description: format!("KDE proxy: {}", proxy_url),
                    });
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Port scanning with HTTP CONNECT probe validation
// ---------------------------------------------------------------------------

/// Send an HTTP CONNECT request through `proxy_addr` and verify the response
/// looks like a functioning proxy. Returns `true` only when the proxy replies
/// with `HTTP/1.x 200`.
///
/// We probe `connectivitycheck.gstatic.com:443` — a lightweight Google
/// endpoint that is globally reachable and rarely blocked, making it a good
/// canary. The entire operation runs within `timeout`.
fn validate_http_proxy(proxy_addr: std::net::SocketAddr, timeout: Duration) -> bool {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    // The target we ask the proxy to CONNECT to. We don't actually complete
    // a TLS handshake — we only need the proxy's `200 Connection established`
    // response to confirm it speaks proxy protocol.
    const PROBE_HOST: &str = "connectivitycheck.gstatic.com";
    const PROBE_PORT: u16 = 443;

    let stream = match TcpStream::connect_timeout(&proxy_addr, timeout) {
        Ok(s) => s,
        Err(_) => return false,
    };

    if stream.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    if stream.set_write_timeout(Some(timeout)).is_err() {
        return false;
    }

    let request = format!(
        "CONNECT {}:{} HTTP/1.0\r\nHost: {}:{}\r\n\r\n",
        PROBE_HOST, PROBE_PORT, PROBE_HOST, PROBE_PORT
    );

    let mut stream = stream;

    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    // Read just enough to check the status line — we don't need the full
    // response headers, and we don't want to wait for a body that won't come.
    let mut buf = [0u8; 64];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return false,
    };

    let response = std::str::from_utf8(&buf[..n]).unwrap_or("");

    // A well-behaved HTTP proxy responds with:
    //   HTTP/1.0 200 Connection established
    //   HTTP/1.1 200 Connection established
    // Non-proxy services (dev servers, databases, etc.) respond with HTML
    // error pages, 400 Bad Request, or nothing at all.
    let valid = response.starts_with("HTTP/1.") && response.contains(" 200 ");

    if valid {
        debug!("proxy: CONNECT probe to {}:{} accepted", PROBE_HOST, PROBE_PORT);
    } else {
        debug!(
            "proxy: CONNECT probe rejected (response prefix: {:?})",
            &response.chars().take(40).collect::<String>()
        );
    }

    valid
}

/// Scan a fixed set of well-known proxy ports on localhost, but only accept
/// a port as a proxy if it passes an HTTP CONNECT validation probe.
///
/// Port 8080 is intentionally excluded because it is used by far too many
/// non-proxy services (dev servers, web UIs, Jupyter, etc.) to be useful as
/// a proxy heuristic.
fn detect_common_proxy_ports() -> Option<ResolvedProxy> {
    use std::net::TcpStream;

    // Ports ordered by likelihood of being a real proxy.
    // 8080 is deliberately omitted — too many false positives.
    let common_ports: &[(u16, &str)] = &[
        (7890, "http"),   // Clash
        (7891, "http"),   // Clash (HTTP)
        (8118, "http"),   // Privoxy
        (3128, "http"),   // Squid
        (1080, "socks5"), // SOCKS5 generic
        (10809, "http"),  // v2rayN HTTP
        (10808, "socks5"), // v2rayN SOCKS
        (2080, "http"),
        (2081, "socks5"),
        (9050, "socks5"), // Tor
        (9150, "socks5"), // Tor Browser
    ];

    // Quick TCP connect timeout — we only want to know if something is
    // listening before paying for the full CONNECT probe round-trip.
    let tcp_timeout = Duration::from_millis(50);

    // Budget for the full CONNECT probe (connect + write + read).
    let probe_timeout = Duration::from_millis(500);

    for &(port, scheme) in common_ports {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

        // Fast path: skip if nothing is listening.
        if TcpStream::connect_timeout(&addr, tcp_timeout).is_err() {
            continue;
        }

        // Something is listening — but is it actually a proxy?
        // Only HTTP(S) proxies support CONNECT; skip the probe for SOCKS ports
        // because SOCKS uses a binary protocol and we can't validate it the
        // same way. For SOCKS we accept the TCP connect as sufficient evidence
        // (SOCKS proxies on those ports are almost exclusively real proxies).
        let is_validated = if scheme == "http" || scheme == "https" {
            validate_http_proxy(addr, probe_timeout)
        } else {
            // For SOCKS ports we trust the well-known port numbers.
            true
        };

        if is_validated {
            let proxy_url = format!("{}://127.0.0.1:{}", scheme, port);
            info!("proxy: Validated proxy on port {} ({})", port, scheme);
            return Some(ResolvedProxy {
                url: proxy_url,
                source: "detected".to_string(),
                description: format!("Detected local proxy on port {}", port),
            });
        } else {
            debug!(
                "proxy: Port {} has a listener but failed CONNECT probe — not treating as proxy",
                port
            );
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Strategy list (used by download engine)
// ---------------------------------------------------------------------------

/// Return an ordered list of proxy strategies to try for a given config.
///
/// When the resolved proxy URL is marked as broken by the circuit breaker,
/// it is skipped and we go straight to "no proxy" — no point hammering a
/// known-broken proxy.
pub fn proxy_strategies(config: &ProxyConfig) -> Vec<(&'static str, Option<String>)> {
    let resolved = resolve_proxy(config);

    match config.mode.as_str() {
        "none" => {
            vec![("no proxy", None)]
        }
        "custom" => {
            if !resolved.url.is_empty() {
                // Skip a broken proxy immediately.
                if is_broken(&resolved.url) {
                    warn!(
                        "proxy: Custom proxy {} is broken (circuit breaker open), using direct",
                        resolved.url
                    );
                    return vec![("no proxy", None)];
                }

                if config.retry_without_proxy {
                    let system_proxy = detect_system_proxy();
                    let mut strategies = vec![("custom proxy", Some(resolved.url.clone()))];
                    if !system_proxy.url.is_empty()
                        && system_proxy.url != resolved.url
                        && !is_broken(&system_proxy.url)
                    {
                        strategies.push(("system proxy", Some(system_proxy.url)));
                    }
                    strategies.push(("no proxy", None));
                    strategies
                } else {
                    vec![("custom proxy", Some(resolved.url))]
                }
            } else if config.retry_without_proxy {
                let system_proxy = detect_system_proxy();
                if !system_proxy.url.is_empty() && !is_broken(&system_proxy.url) {
                    vec![("system proxy", Some(system_proxy.url)), ("no proxy", None)]
                } else {
                    vec![("no proxy", None)]
                }
            } else {
                vec![("no proxy", None)]
            }
        }
        _ => {
            if !resolved.url.is_empty() {
                // Skip a broken detected/system proxy.
                if is_broken(&resolved.url) {
                    warn!(
                        "proxy: System proxy {} is broken (circuit breaker open), using direct",
                        resolved.url
                    );
                    return vec![("no proxy", None)];
                }
                vec![("system proxy", Some(resolved.url)), ("no proxy", None)]
            } else {
                vec![("no proxy", None)]
            }
        }
    }
}
