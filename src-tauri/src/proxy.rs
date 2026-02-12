use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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

fn detect_common_proxy_ports() -> Option<ResolvedProxy> {
    use std::net::TcpStream;

    let common_ports = [
        (7890, "http"),
        (7891, "http"),
        (8080, "http"),
        (8118, "http"),
        (3128, "http"),
        (1080, "socks5"),
        (10809, "http"),
        (10808, "socks5"),
        (2080, "http"),
        (2081, "socks5"),
        (9050, "socks5"),
        (9150, "socks5"),
    ];

    for (port, scheme) in common_ports {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            let proxy_url = format!("{}://127.0.0.1:{}", scheme, port);
            info!("Found open proxy port: {}", port);
            return Some(ResolvedProxy {
                url: proxy_url.clone(),
                source: "detected".to_string(),
                description: format!("Detected local proxy on port {}", port),
            });
        }
    }

    None
}

pub fn proxy_strategies(config: &ProxyConfig) -> Vec<(&'static str, Option<String>)> {
    let resolved = resolve_proxy(config);

    match config.mode.as_str() {
        "none" => {
            vec![("no proxy", None)]
        }
        "custom" => {
            if !resolved.url.is_empty() {
                if config.retry_without_proxy {
                    let system_proxy = detect_system_proxy();
                    let mut strategies = vec![("custom proxy", Some(resolved.url.clone()))];
                    if !system_proxy.url.is_empty() && system_proxy.url != resolved.url {
                        strategies.push(("system proxy", Some(system_proxy.url)));
                    }
                    strategies.push(("no proxy", None));
                    strategies
                } else {
                    vec![("custom proxy", Some(resolved.url))]
                }
            } else if config.retry_without_proxy {
                let system_proxy = detect_system_proxy();
                if !system_proxy.url.is_empty() {
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
                vec![("system proxy", Some(resolved.url)), ("no proxy", None)]
            } else {
                vec![("no proxy", None)]
            }
        }
    }
}
