use crate::backends::{Backend, DownloadBackend, DownloadRequest, InfoRequest, PlaylistRequest};
use crate::cache;
use crate::deps;
use crate::job_engine;
use crate::proxy;
use crate::types::{Chapter, PlaylistEntry, PlaylistInfo, Storyboard, VideoFormat, VideoFormats, VideoInfo};
use async_trait::async_trait;
use log::{debug, error, info};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use url::Url;

#[cfg(target_os = "windows")]
use crate::utils::CommandHideConsole;

pub struct YtDlpBackend;

pub struct CommandConfig {
    pub ytdlp_path: String,
    pub prefix_args: Vec<String>,
    pub env_vars: Vec<(String, String)>,
    pub deno_path: Option<String>,
    pub quickjs_path: Option<String>,
}

pub fn get_command(app: &AppHandle, _proxy_url: Option<&str>) -> Result<CommandConfig, String> {
    let ytdlp_path = deps::get_ytdlp_path(app)?;
    if !ytdlp_path.exists() {
        return Err("yt-dlp is not installed. Please install it first.".to_string());
    }

    let deno_path = deps::get_deno_path(app)?;
    let deno_option = if deno_path.exists() {
        Some(deno_path.to_string_lossy().to_string())
    } else {
        None
    };

    let quickjs_path = deps::get_quickjs_path(app)?;
    let quickjs_option = if quickjs_path.exists() {
        Some(quickjs_path.to_string_lossy().to_string())
    } else {
        None
    };

    let bin_dir = ytdlp_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut env_vars = vec![];

    if !bin_dir.is_empty() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", bin_dir, current_path);
        #[cfg(not(target_os = "windows"))]
        let new_path = format!("{}:{}", bin_dir, current_path);
        env_vars.push(("PATH".to_string(), new_path));
    }

    Ok(CommandConfig {
        ytdlp_path: ytdlp_path.to_string_lossy().to_string(),
        prefix_args: vec![],
        env_vars,
        deno_path: deno_option,
        quickjs_path: quickjs_option,
    })
}

fn apply_site_headers(url: &str, args: &mut Vec<String>) {
    // Some sites (notably bilibili) block yt-dlp's default python/urllib user agent.
    // Use a stable modern browser UA so initial webpage fetch isn't rejected.
    args.extend([
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
    ]);

    let is_bilibili = url.contains("bilibili.com") || url.contains("b23.tv") || url.contains("bilivideo.com");
    if is_bilibili {
        args.extend([
            "--referer".to_string(),
            "https://www.bilibili.com/".to_string(),
            "--add-header".to_string(),
            "Origin:https://www.bilibili.com".to_string(),
            "--add-header".to_string(),
            "Accept-Language:zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7".to_string(),
        ]);
    }
}

fn normalize_url_for_ytdlp(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };

    let host = parsed.host_str().unwrap_or("").to_lowercase();

    // Vimeo private/unlisted/review links are often shared in the form:
    //   https://vimeo.com/<id>/<hash>
    // In practice, the player endpoint is the most reliable canonical form for yt-dlp:
    //   https://player.vimeo.com/video/<id>?h=<hash>
    // Some link types may 404 on https://vimeo.com/<id>?h=<hash>, so prefer the player URL.
    let is_vimeo = host == "vimeo.com" || host.ends_with(".vimeo.com");
    if is_vimeo {
        let has_h = parsed
            .query_pairs()
            .any(|(k, _)| k.eq_ignore_ascii_case("h"));

        if !has_h {
            let segments: Vec<String> = parsed
                .path_segments()
                .map(|s| s.map(|p| p.to_string()).collect())
                .unwrap_or_default();

            // https://vimeo.com/<id>/<hash>
            if host == "vimeo.com"
                && segments.len() >= 2
                && segments[0].chars().all(|c| c.is_ascii_digit())
                && !segments[1].is_empty()
                && segments[1].chars().all(|c| c.is_ascii_hexdigit())
            {
                let id = &segments[0];
                let hash = &segments[1];
                let Ok(mut player) = Url::parse(&format!("https://player.vimeo.com/video/{}", id)) else {
                    return url.to_string();
                };

                // Preserve any existing query params (rare, but safe), then add `h`.
                let mut qp: Vec<(String, String)> = parsed
                    .query_pairs()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                qp.push(("h".to_string(), hash.to_string()));

                {
                    let mut pairs = player.query_pairs_mut();
                    for (k, v) in qp {
                        pairs.append_pair(&k, &v);
                    }
                }

                return player.to_string();
            }

            // https://player.vimeo.com/video/<id>/<hash>
            if host == "player.vimeo.com"
                && segments.len() >= 3
                && segments[0] == "video"
                && segments[1].chars().all(|c| c.is_ascii_digit())
                && !segments[2].is_empty()
                && segments[2].chars().all(|c| c.is_ascii_hexdigit())
            {
                let id = &segments[1];
                let hash = &segments[2];
                parsed.set_path(&format!("/video/{}", id));
                let mut qp: Vec<(String, String)> = parsed
                    .query_pairs()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                qp.push(("h".to_string(), hash.to_string()));
                parsed.set_query(None);
                {
                    let mut pairs = parsed.query_pairs_mut();
                    for (k, v) in qp {
                        pairs.append_pair(&k, &v);
                    }
                }
                return parsed.to_string();
            }
        }
    }

    url.to_string()
}

pub async fn setup_cookies(
    app: &AppHandle,
    args: &mut Vec<String>,
    cookies_from_browser: &Option<String>,
    custom_cookies: &Option<String>,
) -> Result<bool, String> {
    // Only use custom cookies if cookiesFromBrowser is explicitly set to "custom"
    let use_custom_cookies = cookies_from_browser
        .as_ref()
        .map(|s| s == "custom")
        .unwrap_or(false)
        && custom_cookies
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);

    if use_custom_cookies {
        if let Some(cookies_text) = custom_cookies.as_deref() {
            let cache_dir = app
                .path()
                .app_cache_dir()
                .map_err(|e| format!("Failed to get cache dir: {}", e))?;
            let cookies_file = cache_dir.join("custom_cookies.txt");

            tokio::fs::create_dir_all(&cache_dir)
                .await
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;

            tokio::fs::write(&cookies_file, cookies_text)
                .await
                .map_err(|e| format!("Failed to write cookies file: {}", e))?;

            args.push("--cookies".to_string());
            args.push(cookies_file.to_string_lossy().to_string());
            info!("Using custom cookies file: {:?}", cookies_file);
        }
    } else if let Some(ref browser) = cookies_from_browser {
        if !browser.is_empty() && browser != "custom" {
            args.push("--cookies-from-browser".to_string());
            args.push(browser.clone());
            info!("Using cookies from browser: {}", browser);
        }
    }

    Ok(use_custom_cookies)
}

async fn cleanup_custom_cookies(app: &AppHandle) {
    if let Ok(cache_dir) = app.path().app_cache_dir() {
        let _ = tokio::fs::remove_file(cache_dir.join("custom_cookies.txt")).await;
    }
}

#[async_trait]
impl Backend for YtDlpBackend {
    async fn get_video_info(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoInfo, String> {
        if let Some(cached) = cache::get_cached_video_info(&request.url) {
            info!("Video info cache hit for URL: {}", request.url);
            return Ok(cached);
        }

        let resolved_proxy = request.proxy_config.as_ref().map(proxy::resolve_proxy);
        let proxy_url = resolved_proxy.as_ref().and_then(|p| {
            if p.url.is_empty() {
                None
            } else {
                Some(p.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args: Vec<String> = config.prefix_args;
        args.extend([
            "--encoding".to_string(),
            "utf-8".to_string(),
            "--print".to_string(),
            "%(title)s".to_string(),
            "--print".to_string(),
            "%(uploader)s".to_string(),
            "--print".to_string(),
            "%(channel)s".to_string(),
            "--print".to_string(),
            "%(creator)s".to_string(),
            "--print".to_string(),
            "%(uploader_id)s".to_string(),
            "--print".to_string(),
            "%(thumbnail)s".to_string(),
            "--print".to_string(),
            "%(duration)s".to_string(),
            "--no-download".to_string(),
            "--no-playlist".to_string(),
            "--flat-playlist".to_string(),
        ]);

        if let Some(p) = proxy_url {
            args.extend(["--proxy".to_string(), p.to_string()]);
            info!("Using --proxy argument for video info: {}", p);
        }

        let use_custom_cookies = setup_cookies(
            app,
            &mut args,
            &request.cookies_from_browser,
            &request.custom_cookies,
        )
        .await?;

        let is_youtube = request.url.contains("youtube.com") || request.url.contains("youtu.be");
        if is_youtube {
            // When using cookies, avoid android_sdkless as it doesn't support cookies
            let has_cookies = use_custom_cookies
                || request
                    .cookies_from_browser
                    .as_ref()
                    .map(|s| !s.is_empty() && s != "custom")
                    .unwrap_or(false);

            if !has_cookies {
                let player_client = request
                    .youtube_player_client
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("android_sdkless");
                args.extend([
                    "--extractor-args".to_string(),
                    format!("youtube:player_client={};player_skip=webpage,configs", player_client),
                ]);
            }
        }

        let mut js_runtimes = Vec::new();
        if let Some(ref deno_path) = config.deno_path {
            js_runtimes.push(format!("deno:{}", deno_path));
        }
        if let Some(ref qjs_path) = config.quickjs_path {
            js_runtimes.push(format!("quickjs:{}", qjs_path));
        }
        if !js_runtimes.is_empty() {
            args.extend(["--js-runtimes".to_string(), js_runtimes.join(",")]);
        }

        let ytdlp_url = normalize_url_for_ytdlp(&request.url);
        apply_site_headers(&ytdlp_url, &mut args);
        args.push(ytdlp_url);

        let mut cmd = tokio::process::Command::new(&config.ytdlp_path);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().await.map_err(|e| {
            error!("Failed to get video info: {}", e);
            format!("Failed to get video info: {}", e)
        })?;

        if use_custom_cookies {
            cleanup_custom_cookies(app).await;
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("yt-dlp error: {}", stderr);
            return Err(format!("Failed to get video info: {}", stderr));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = output_str.lines().collect();

        let parse_field = |idx: usize| -> Option<String> {
            lines.get(idx).and_then(|s| {
                if s.is_empty() || *s == "NA" {
                    None
                } else {
                    Some(s.to_string())
                }
            })
        };

        let info = VideoInfo {
            title: lines
                .first()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            uploader: parse_field(1),
            channel: parse_field(2),
            creator: parse_field(3),
            uploader_id: parse_field(4),
            thumbnail: parse_field(5),
            duration: lines.get(6).and_then(|s| s.parse::<f64>().ok()),
            filesize: None,
            ext: None,
        };

        cache::put_video_info(request.url, info.clone());
        Ok(info)
    }

    async fn get_playlist_info(
        &self,
        app: &AppHandle,
        request: PlaylistRequest,
    ) -> Result<PlaylistInfo, String> {
        if let Some(cached) = cache::get_cached_playlist_info(&request.url) {
            info!(
                "Playlist info cache hit for URL: {} (offset={}, limit={})",
                request.url, request.offset, request.limit
            );
            let paginated_entries: Vec<PlaylistEntry> = cached
                .entries
                .iter()
                .skip(request.offset)
                .take(request.limit)
                .cloned()
                .collect();
            let has_more = request.offset + paginated_entries.len() < cached.total_count;

            return Ok(PlaylistInfo {
                is_playlist: cached.is_playlist,
                id: cached.id.clone(),
                title: cached.title.clone(),
                uploader: cached.uploader.clone(),
                thumbnail: cached.thumbnail.clone(),
                total_count: cached.total_count,
                entries: paginated_entries,
                has_more,
            });
        }

        let resolved_proxy = request.proxy_config.as_ref().map(proxy::resolve_proxy);
        let proxy_url = resolved_proxy.as_ref().and_then(|p| {
            if p.url.is_empty() {
                None
            } else {
                Some(p.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args: Vec<String> = config.prefix_args;
        args.extend([
            "--encoding".to_string(),
            "utf-8".to_string(),
            "--dump-json".to_string(),
            "--flat-playlist".to_string(),
            "--no-download".to_string(),
        ]);

        if let Some(p) = proxy_url {
            args.extend(["--proxy".to_string(), p.to_string()]);
            info!("Using --proxy argument for playlist info: {}", p);
        }

        let use_custom_cookies = setup_cookies(
            app,
            &mut args,
            &request.cookies_from_browser,
            &request.custom_cookies,
        )
        .await?;

        let is_youtube = request.url.contains("youtube.com") || request.url.contains("youtu.be");
        if is_youtube {
            // When using cookies, avoid android_sdkless as it doesn't support cookies
            let has_cookies = use_custom_cookies
                || request
                    .cookies_from_browser
                    .as_ref()
                    .map(|s| !s.is_empty() && s != "custom")
                    .unwrap_or(false);

            if !has_cookies {
                let player_client = request
                    .youtube_player_client
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("android_sdkless");
                args.extend([
                    "--extractor-args".to_string(),
                    format!("youtube:player_client={};player_skip=webpage,configs", player_client),
                ]);
            }
        }

        let mut js_runtimes = Vec::new();
        if let Some(ref deno_path) = config.deno_path {
            js_runtimes.push(format!("deno:{}", deno_path));
        }
        if let Some(ref qjs_path) = config.quickjs_path {
            js_runtimes.push(format!("quickjs:{}", qjs_path));
        }
        if !js_runtimes.is_empty() {
            args.extend(["--js-runtimes".to_string(), js_runtimes.join(",")]);
        }

        let ytdlp_url = normalize_url_for_ytdlp(&request.url);
        apply_site_headers(&ytdlp_url, &mut args);
        args.push(ytdlp_url);

        let mut cmd = tokio::process::Command::new(&config.ytdlp_path);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().await.map_err(|e| {
            error!("Failed to get playlist info: {}", e);
            format!("Failed to get playlist info: {}", e)
        })?;

        if use_custom_cookies {
            cleanup_custom_cookies(app).await;
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("yt-dlp error: {}", stderr);
            return Err(format!("Failed to get playlist info: {}", stderr));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&json_str);

        let (json, entries_from_lines): (Option<serde_json::Value>, Vec<serde_json::Value>) =
            match parse_result {
                Ok(single_json) => (Some(single_json), vec![]),
                Err(_) => {
                    let entries: Vec<serde_json::Value> = json_str
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .filter_map(|line| serde_json::from_str(line).ok())
                        .collect();

                    if entries.is_empty() {
                        return Err(
                            "Failed to parse playlist info: no valid JSON found".to_string()
                        );
                    }

                    (None, entries)
                }
            };

        let (is_playlist, playlist_json, is_ndjson_format, all_entries) = if let Some(ref single) =
            json
        {
            let is_pl = single.get("_type").and_then(|v| v.as_str()) == Some("playlist");
            if is_pl {
                let entries = single["entries"].as_array().cloned().unwrap_or_default();
                (true, Some(single.clone()), false, entries)
            } else {
                (false, Some(single.clone()), false, vec![single.clone()])
            }
        } else {
            let first = entries_from_lines.first();
            let is_pl = entries_from_lines.len() > 1
                || first.and_then(|f| f.get("_type")).and_then(|v| v.as_str()) == Some("playlist");
            (is_pl, first.cloned(), true, entries_from_lines)
        };

        if !is_playlist && all_entries.len() == 1 {
            let video = &all_entries[0];
            let is_ytm = request.url.contains("music.youtube.com");
            return Ok(PlaylistInfo {
                is_playlist: false,
                id: video["id"].as_str().map(|s| s.to_string()),
                title: video["title"].as_str().unwrap_or("Unknown").to_string(),
                uploader: video["uploader"]
                    .as_str()
                    .or(video["channel"].as_str())
                    .map(|s| s.to_string()),
                thumbnail: video["thumbnail"].as_str().map(|s| s.to_string()),
                total_count: 1,
                entries: vec![PlaylistEntry {
                    id: video["id"].as_str().unwrap_or("").to_string(),
                    url: request.url.clone(),
                    title: video["title"].as_str().unwrap_or("Unknown").to_string(),
                    duration: video["duration"].as_f64(),
                    thumbnail: video["thumbnail"].as_str().map(|s| s.to_string()),
                    uploader: video["uploader"].as_str().map(|s| s.to_string()),
                    is_music: is_ytm,
                }],
                has_more: false,
            });
        }

        let total_count = all_entries.len();
        let is_ytm_playlist = request.url.contains("music.youtube.com");

        let all_processed_entries: Vec<PlaylistEntry> = all_entries
            .iter()
            .filter_map(|entry| {
                let id = entry["id"].as_str()?.to_string();
                let title = entry["title"].as_str().unwrap_or("Unknown").to_string();
                let duration = entry["duration"].as_f64();
                let is_music = is_ytm_playlist || duration.map(|d| d < 600.0).unwrap_or(false);

                let entry_url =
                    if request.url.contains("youtube.com") || request.url.contains("youtu.be") {
                        if is_ytm_playlist {
                            format!("https://music.youtube.com/watch?v={}", id)
                        } else {
                            format!("https://www.youtube.com/watch?v={}", id)
                        }
                    } else {
                        entry["url"].as_str().unwrap_or("").to_string()
                    };

                Some(PlaylistEntry {
                    id,
                    url: entry_url,
                    title,
                    duration,
                    thumbnail: entry["thumbnail"]
                        .as_str()
                        .or(entry["thumbnails"]
                            .as_array()
                            .and_then(|t| t.first())
                            .and_then(|t| t["url"].as_str()))
                        .map(|s| s.to_string()),
                    uploader: entry["uploader"]
                        .as_str()
                        .or(entry["channel"].as_str())
                        .map(|s| s.to_string()),
                    is_music,
                })
            })
            .collect();

        let paginated_entries: Vec<PlaylistEntry> = all_processed_entries
            .iter()
            .skip(request.offset)
            .take(request.limit)
            .cloned()
            .collect();

        let has_more = request.offset + paginated_entries.len() < total_count;

        let playlist_title = if is_ndjson_format {
            all_entries
                .first()
                .and_then(|e| e["playlist_title"].as_str())
                .map(|s| s.to_string())
        } else {
            playlist_json
                .as_ref()
                .and_then(|pj| pj["title"].as_str().map(|s| s.to_string()))
        };

        let playlist_id = if is_ndjson_format {
            all_entries
                .first()
                .and_then(|e| e["playlist_id"].as_str())
                .map(|s| s.to_string())
        } else {
            playlist_json
                .as_ref()
                .and_then(|pj| pj["id"].as_str().map(|s| s.to_string()))
        };

        let playlist_uploader = if is_ndjson_format {
            all_entries
                .first()
                .and_then(|e| e["playlist_uploader"].as_str().or(e["channel"].as_str()))
                .map(|s| s.to_string())
        } else {
            playlist_json.as_ref().and_then(|pj| {
                pj["uploader"]
                    .as_str()
                    .or(pj["channel"].as_str())
                    .map(|s| s.to_string())
            })
        };

        let result = PlaylistInfo {
            is_playlist: true,
            id: playlist_id,
            title: playlist_title.unwrap_or_else(|| "Playlist".to_string()),
            uploader: playlist_uploader,
            thumbnail: playlist_json.as_ref().and_then(|m| {
                m["thumbnail"]
                    .as_str()
                    .or(m["thumbnails"]
                        .as_array()
                        .and_then(|t| t.first())
                        .and_then(|t| t["url"].as_str()))
                    .map(|s| s.to_string())
            }),
            total_count,
            entries: paginated_entries,
            has_more,
        };

        let cache_entry = PlaylistInfo {
            entries: all_processed_entries,
            has_more: false,
            ..result.clone()
        };
        cache::put_playlist_info(request.url, cache_entry);

        Ok(result)
    }

    async fn get_video_formats(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoFormats, String> {
        if let Some(cached) = cache::get_cached_video_formats(&request.url) {
            info!("Video formats cache hit for URL: {}", request.url);
            return Ok(cached);
        }

        let resolved_proxy = request.proxy_config.as_ref().map(proxy::resolve_proxy);
        let proxy_url = resolved_proxy.as_ref().and_then(|p| {
            if p.url.is_empty() {
                None
            } else {
                Some(p.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args: Vec<String> = config.prefix_args;
        args.extend([
            "--encoding".to_string(),
            "utf-8".to_string(),
            "--dump-json".to_string(),
            "--no-download".to_string(),
            "--no-playlist".to_string(),
            "--flat-playlist".to_string(),
        ]);

        if let Some(p) = proxy_url {
            args.extend(["--proxy".to_string(), p.to_string()]);
        }

        let use_custom_cookies = setup_cookies(
            app,
            &mut args,
            &request.cookies_from_browser,
            &request.custom_cookies,
        )
        .await?;

        let is_youtube = request.url.contains("youtube.com") || request.url.contains("youtu.be");
        if is_youtube {
            // When using cookies, avoid android_sdkless as it doesn't support cookies
            let has_cookies = use_custom_cookies
                || request
                    .cookies_from_browser
                    .as_ref()
                    .map(|s| !s.is_empty() && s != "custom")
                    .unwrap_or(false);

            if !has_cookies {
                let player_client = request
                    .youtube_player_client
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("android_sdkless");
                args.extend([
                    "--extractor-args".to_string(),
                    format!("youtube:player_client={};player_skip=webpage,configs", player_client),
                ]);
            }
        }

        let mut js_runtimes = Vec::new();
        if let Some(ref deno_path) = config.deno_path {
            js_runtimes.push(format!("deno:{}", deno_path));
        }
        if let Some(ref qjs_path) = config.quickjs_path {
            js_runtimes.push(format!("quickjs:{}", qjs_path));
        }
        if !js_runtimes.is_empty() {
            args.extend(["--js-runtimes".to_string(), js_runtimes.join(",")]);
        }

        let ytdlp_url = normalize_url_for_ytdlp(&request.url);
        apply_site_headers(&ytdlp_url, &mut args);
        args.push(ytdlp_url);

        info!("Running yt-dlp with args: {:?}", args);

        let mut cmd = tokio::process::Command::new(&config.ytdlp_path);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().await.map_err(|e| {
            error!("Failed to get video formats: {}", e);
            format!("Failed to get video formats: {}", e)
        })?;

        if use_custom_cookies {
            cleanup_custom_cookies(app).await;
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("yt-dlp error: {}", stderr);
            return Err(format!("Failed to get video formats: {}", stderr));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let title = json["title"].as_str().unwrap_or("Unknown").to_string();
        let author = json["uploader"]
            .as_str()
            .or_else(|| json["channel"].as_str())
            .or_else(|| json["artist"].as_str())
            .map(|s| s.strip_suffix(" - Topic").unwrap_or(s).to_string());
        let thumbnail = json["thumbnail"].as_str().map(|s| s.to_string());
        let duration = json["duration"].as_f64();

        let formats_json = json["formats"].as_array().ok_or("No formats found")?;

        let formats: Vec<VideoFormat> = formats_json
            .iter()
            .filter_map(|f| {
                let format_id = f["format_id"].as_str()?.to_string();
                let ext = f["ext"].as_str().unwrap_or("unknown").to_string();

                if ext == "mhtml" || format_id.contains("storyboard") {
                    return None;
                }

                let vcodec = f["vcodec"].as_str().map(|s| s.to_string());
                let acodec = f["acodec"].as_str().map(|s| s.to_string());

                let has_video = vcodec.as_ref().map(|v| v != "none").unwrap_or(false);
                let has_audio = acodec.as_ref().map(|a| a != "none").unwrap_or(false);

                if !has_video && !has_audio {
                    return None;
                }

                let resolution = if has_video {
                    let width = f["width"].as_u64();
                    let height = f["height"].as_u64();
                    match (width, height) {
                        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                        _ => f["resolution"].as_str().map(|s| s.to_string()),
                    }
                } else {
                    Some("audio only".to_string())
                };

                Some(VideoFormat {
                    format_id,
                    ext,
                    resolution,
                    fps: f["fps"].as_f64(),
                    vcodec: if has_video { vcodec } else { None },
                    acodec: if has_audio { acodec } else { None },
                    filesize: f["filesize"].as_u64(),
                    filesize_approx: f["filesize_approx"].as_u64(),
                    tbr: f["tbr"].as_f64(),
                    vbr: f["vbr"].as_f64(),
                    abr: f["abr"].as_f64(),
                    asr: f["asr"].as_u64().map(|v| v as u32),
                    format_note: f["format_note"].as_str().map(|s| s.to_string()),
                    has_video,
                    has_audio,
                    quality: f["quality"].as_f64(),
                })
            })
            .collect();

        info!("Found {} formats for {}", formats.len(), request.url);

        // Parse storyboards from formats (YouTube provides these as special format entries).
        let format_ids: Vec<&str> = formats_json
            .iter()
            .filter_map(|f| f["format_id"].as_str())
            .collect();
        debug!("All format IDs: {:?}", format_ids);
        
        let storyboards_vec: Vec<Storyboard> = formats_json
            .iter()
            .filter_map(|f| {
                let format_id = f["format_id"].as_str()?;
                let format_note = f["format_note"].as_str().unwrap_or("");
                let ext = f["ext"].as_str().unwrap_or("");
                
                // YouTube storyboards typically have format_id like "sb0", "sb1", etc.
                let is_storyboard = format_id.starts_with("sb") || 
                                    format_note.to_lowercase().contains("storyboard") ||
                                    (ext == "mhtml" && f["fragments"].is_array());
                
                if !is_storyboard {
                    return None;
                }
                
                debug!("Found storyboard format: {} (note: {}, ext: {})", format_id, format_note, ext);
                
                // YouTube storyboards have fragments array
                let fragments = f["fragments"].as_array();
                let fragment_count = fragments.map(|f| f.len()).unwrap_or(0) as u32;
                
                if fragment_count == 0 {
                    debug!("Storyboard has no fragments, skipping");
                    return None;
                }
                
                // Get dimensions - these are per-cell dimensions in newer yt-dlp
                // width/height might be total sprite sheet size or per-cell size
                let width = f["width"].as_u64().unwrap_or(160) as u32;
                let height = f["height"].as_u64().unwrap_or(90) as u32;
                let cols = f["columns"].as_u64().unwrap_or(10) as u32;
                let rows = f["rows"].as_u64().unwrap_or(10) as u32;
                
                debug!("Storyboard dims: {}x{}, grid: {}x{}, {} fragments", width, height, cols, rows, fragment_count);
                
                // fragment_duration is how much time ONE ENTIRE SPRITE SHEET covers
                let fragment_duration = fragments
                    .and_then(|frags| frags.first())
                    .and_then(|frag| frag["duration"].as_f64())
                    .unwrap_or(2.0);
                
                // Get the URL - prefer the format URL which has $M placeholder
                let mut url = f["url"].as_str()
                    .or_else(|| {
                        fragments.and_then(|frags| frags.first()).and_then(|frag| frag["url"].as_str())
                    })?
                    .to_string();

                // Android blocks cleartext (http://) by default, and protocol-relative (//) URLs
                // will inherit the app scheme (e.g., tauri://) which breaks loading.
                if url.starts_with("//") {
                    url = format!("https:{}", url);
                } else if url.starts_with("http://") {
                    url = url.replacen("http://", "https://", 1);
                }
                
                debug!("Storyboard URL template: {}", &url[..url.len().min(100)]);
                
                Some(Storyboard {
                    url,
                    width,
                    height,
                    cols,
                    rows,
                    fragment_count,
                    fragment_duration,
                })
            })
            .collect();
        
        // Sort storyboards by resolution (width * height) descending - best quality first
        let mut storyboards_sorted = storyboards_vec;
        storyboards_sorted.sort_by(|a, b| {
            let res_a = (a.width * a.height) as i64;
            let res_b = (b.width * b.height) as i64;
            res_b.cmp(&res_a)
        });
        
        let storyboards = if storyboards_sorted.is_empty() { None } else { Some(storyboards_sorted) };
        if let Some(ref sb) = storyboards {
            info!("Total {} storyboard format(s) found, best: {}x{}", sb.len(), sb[0].width, sb[0].height);
        }

        // Parse chapters if available
        let chapters: Option<Vec<Chapter>> = json["chapters"]
            .as_array()
            .map(|chapters_arr| {
                chapters_arr
                    .iter()
                    .filter_map(|ch| {
                        let title = ch["title"].as_str()?.to_string();
                        let start_time = ch["start_time"].as_f64()?;
                        let end_time = ch["end_time"].as_f64()?;
                        Some(Chapter {
                            title,
                            start_time,
                            end_time,
                        })
                    })
                    .collect()
            })
            .filter(|v: &Vec<Chapter>| !v.is_empty());
        
        if let Some(ref ch) = chapters {
            info!("Found {} chapter(s)", ch.len());
        }

        let result = VideoFormats {
            title,
            author,
            thumbnail,
            duration,
            formats,
            view_count: json["view_count"].as_u64(),
            like_count: json["like_count"].as_u64(),
            description: json["description"].as_str().map(|s| s.to_string()),
            upload_date: json["upload_date"].as_str().map(|s| s.to_string()),
            channel_url: json["channel_url"]
                .as_str()
                .or_else(|| json["uploader_url"].as_str())
                .map(|s| s.to_string()),
            channel_id: json["channel_id"]
                .as_str()
                .or_else(|| json["uploader_id"].as_str())
                .map(|s| s.to_string()),
            storyboards,
            chapters,
        };

        cache::put_video_formats(request.url, result.clone());
        Ok(result)
    }
}

#[async_trait]
impl DownloadBackend for YtDlpBackend {
    async fn download_job(
        &self,
        app: &AppHandle,
        window: tauri::Window,
        registry: crate::job_engine::JobRegistry,
        request: DownloadRequest,
    ) -> Result<String, String> {
        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(proxy::resolve_proxy);
        let proxy_url = resolved_proxy.as_ref().and_then(|r| {
            if r.url.is_empty() {
                None
            } else {
                Some(r.url.as_str())
            }
        });

        if let Some(ref proxy) = resolved_proxy {
            if !proxy.url.is_empty() {
                info!("Using proxy for download: {} ({})", proxy.url, proxy.source);
            }
        }

        let config = get_command(app, proxy_url)?;

        debug!(
            "Using command: {} with prefix args: {:?}",
            config.ytdlp_path, config.prefix_args
        );

        let mut downloads_dir = if let Some(ref custom_path) = request.download_path {
            if !custom_path.is_empty() {
                std::path::PathBuf::from(custom_path)
            } else {
                dirs::download_dir().ok_or("Could not find Downloads folder")?
            }
        } else {
            dirs::download_dir().ok_or("Could not find Downloads folder")?
        };

        if let Some(ref title) = request.playlist_title {
            if !title.is_empty() {
                let safe_folder_name: String = title
                    .chars()
                    .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(100)
                    .collect();
                if !safe_folder_name.is_empty() {
                    downloads_dir = downloads_dir.join(&safe_folder_name);
                    info!("Using playlist subfolder: {:?}", downloads_dir);
                }
            }
        }

        if !downloads_dir.exists() {
            std::fs::create_dir_all(&downloads_dir)
                .map_err(|e| format!("Failed to create download directory: {}", e))?;
            info!("Created download directory: {:?}", downloads_dir);
        }

        let download_mode = request
            .download_mode
            .unwrap_or_else(|| "auto".to_string());
        let user_template = request
            .output_template
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let template = user_template.unwrap_or("%(title)s.%(ext)s");
        let output_template = downloads_dir
            .join(template)
            .to_str()
            .ok_or("Invalid path")?
            .to_string();

        info!("Output template: {}", output_template);
        info!(
            "Download mode: {}, Video quality: {:?}, Audio quality: {:?}",
            download_mode, request.video_quality, request.audio_quality
        );

        let mut args: Vec<String> = config.prefix_args;
        args.extend([
            "--encoding".to_string(),
            "utf-8".to_string(),
            "-o".to_string(),
            output_template.clone(),
            "--newline".to_string(),
            "--progress".to_string(),
            "--progress-template".to_string(),
            "__COMINE_PROGRESS__ %(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s %(progress.downloaded_bytes)s %(progress.total_bytes)s".to_string(),
            "--print".to_string(),
            "after_move:>>>FILEPATH:%(filepath)s".to_string(),
            "--verbose".to_string(),
        ]);

        if let Some(proxy) = proxy_url {
            args.extend(["--proxy".to_string(), proxy.to_string()]);
            info!("Using --proxy argument: {}", proxy);
        }

        if let Some(ref qjs_path) = config.quickjs_path {
            args.extend([
                "--js-runtimes".to_string(),
                format!("quickjs:{}", qjs_path),
            ]);
            info!("Using QuickJS runtime: {}", qjs_path);
        }

        let video_quality = request
            .video_quality
            .unwrap_or_else(|| "max".to_string());
        let audio_quality = request
            .audio_quality
            .unwrap_or_else(|| "best".to_string());

        let is_raw_format = video_quality
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            || video_quality.contains('+')
            || video_quality.starts_with("best");

        let format_string = if is_raw_format {
            info!("Using raw format ID: {}", video_quality);
            video_quality.clone()
        } else {
            match download_mode.as_str() {
                "audio" => match audio_quality.as_str() {
                    "320" => "bestaudio[abr<=320]/bestaudio/best".to_string(),
                    "256" => "bestaudio[abr<=256]/bestaudio/best".to_string(),
                    "192" => "bestaudio[abr<=192]/bestaudio/best".to_string(),
                    "128" => "bestaudio[abr<=128]/bestaudio/best".to_string(),
                    "96" => "bestaudio[abr<=96]/bestaudio/best".to_string(),
                    _ => "bestaudio/best".to_string(),
                },
                "mute" => match video_quality.as_str() {
                    "4k" => "bestvideo[height<=2160]/bestvideo/best".to_string(),
                    "1440p" => "bestvideo[height<=1440]/bestvideo/best".to_string(),
                    "1080p" => "bestvideo[height<=1080]/bestvideo/best".to_string(),
                    "720p" => "bestvideo[height<=720]/bestvideo/best".to_string(),
                    "480p" => "bestvideo[height<=480]/bestvideo/best".to_string(),
                    "360p" => "bestvideo[height<=360]/bestvideo/best".to_string(),
                    "240p" => "bestvideo[height<=240]/bestvideo/best".to_string(),
                    _ => "bestvideo/best".to_string(),
                },
                _ => match video_quality.as_str() {
                    "4k" => "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best".to_string(),
                    "1440p" => "bestvideo[height<=1440]+bestaudio/best[height<=1440]/best".to_string(),
                    "1080p" => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best".to_string(),
                    "720p" => "bestvideo[height<=720]+bestaudio/best[height<=720]/best".to_string(),
                    "480p" => "bestvideo[height<=480]+bestaudio/best[height<=480]/best".to_string(),
                    "360p" => "bestvideo[height<=360]+bestaudio/best[height<=360]/best".to_string(),
                    "240p" => "bestvideo[height<=240]+bestaudio/best[height<=240]/best".to_string(),
                    _ => "bestvideo+bestaudio/best".to_string(),
                },
            }
        };

        args.extend(["-f".to_string(), format_string.clone()]);
        info!("Using format: {}", format_string);

        if download_mode == "audio" {
            args.extend(["-x".to_string(), "--audio-format".to_string(), "m4a".to_string()]);

            let has_thumb_url_for_embed = request
                .thumbnail_url_for_embed
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            let should_manual_embed_ytm = request.embed_thumbnail.unwrap_or(false)
                && request.url.to_lowercase().contains("music.youtube.com")
                && has_thumb_url_for_embed;

            if request.embed_thumbnail.unwrap_or(false) && !should_manual_embed_ytm {
                args.push("--embed-thumbnail".to_string());
                info!("Embedding thumbnail as cover art (via yt-dlp)");
            } else if should_manual_embed_ytm {
                info!("Will embed YTM thumbnail manually after download");
            }

            info!("Audio-only download with extraction (ffprobe available)");
        }

        if download_mode != "audio" {
            if request.convert_to_mp4.unwrap_or(false) {
                args.extend([
                    "--format-sort".to_string(),
                    "vcodec:h264,acodec:aac".to_string(),
                ]);
                args.extend(["--recode-video".to_string(), "mp4".to_string()]);
                info!("Converting to MP4 (with h264/aac preference to minimize re-encoding)");
            } else if request.remux.unwrap_or(true) {
                args.extend(["--remux-video".to_string(), "mp4".to_string()]);
                info!("Remuxing to MP4 (copy, no re-encode)");
            }
        }

        if request.clear_metadata.unwrap_or(false) {
            args.push("--no-embed-metadata".to_string());
            info!("Clearing metadata");
        }

        if request.no_playlist.unwrap_or(true) {
            args.push("--no-playlist".to_string());
            info!("Using --no-playlist (single video only)");
        }

        let use_custom_cookies = request
            .cookies_from_browser
            .as_ref()
            .map(|s| s == "custom")
            .unwrap_or(false)
            && request
                .custom_cookies
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

        if use_custom_cookies {
            if let Some(cookies_text) = request.custom_cookies.as_deref() {
                let cache_dir = app
                    .path()
                    .app_cache_dir()
                    .map_err(|e| format!("Failed to get cache dir: {}", e))?;
                let cookies_file = cache_dir.join("custom_cookies.txt");

                tokio::fs::create_dir_all(&cache_dir)
                    .await
                    .map_err(|e| format!("Failed to create cache dir: {}", e))?;

                tokio::fs::write(&cookies_file, cookies_text)
                    .await
                    .map_err(|e| format!("Failed to write cookies file: {}", e))?;

                args.push("--cookies".to_string());
                args.push(cookies_file.to_string_lossy().to_string());
                info!("Using custom cookies file: {:?}", cookies_file);
            }
        } else if let Some(ref browser) = request.cookies_from_browser {
            if !browser.is_empty() && browser != "custom" {
                args.push("--cookies-from-browser".to_string());
                args.push(browser.clone());
                info!("Using cookies from browser: {}", browser);
            }
        }

        let is_youtube = request.url.contains("youtube.com") || request.url.contains("youtu.be");
        if is_youtube {
            let has_cookies = use_custom_cookies
                || request
                    .cookies_from_browser
                    .as_ref()
                    .map(|s| !s.is_empty() && s != "custom")
                    .unwrap_or(false);

            let default_client = if has_cookies { "tv,web" } else { "tv,android_sdkless" };
            let player_client = request
                .youtube_player_client
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(default_client);

            let final_client = if has_cookies && player_client.contains("android_sdkless") {
                player_client.replace("android_sdkless", "tv")
            } else {
                player_client.to_string()
            };

            args.extend([
                "--extractor-args".to_string(),
                format!("youtube:player_client={}", final_client),
            ]);
            info!("Using player client chain for YouTube: {}", final_client);
        }

        if request.sponsor_block.unwrap_or(false) {
            let mut categories: Vec<&str> = Vec::new();
            if request.sponsor_block_skip_sponsors.unwrap_or(true) {
                categories.push("sponsor");
            }
            if request.sponsor_block_skip_intros.unwrap_or(false) {
                categories.push("intro");
                categories.push("outro");
            }
            if request.sponsor_block_skip_self_promo.unwrap_or(false) {
                categories.push("selfpromo");
            }
            if request.sponsor_block_skip_interaction.unwrap_or(false) {
                categories.push("interaction");
            }

            if !categories.is_empty() {
                args.extend(["--sponsorblock-remove".to_string(), categories.join(",")]);
                info!("SponsorBlock enabled - removing: {}", categories.join(", "));
            } else {
                info!("SponsorBlock enabled - no categories selected");
            }
        }

        if request.chapters.unwrap_or(true) {
            args.push("--embed-chapters".to_string());
            info!("Embedding chapters");
        }

        if request.embed_subtitles.unwrap_or(false) {
            let langs = request.subtitle_languages.as_deref().unwrap_or("en.*,ru.*");
            args.extend([
                "--embed-subs".to_string(),
                "--sub-langs".to_string(),
                langs.to_string(),
            ]);
            info!("Embedding subtitles ({})", langs);
        }

        if let Some(limit) = request.download_speed_limit {
            if limit > 0 {
                args.extend(["--limit-rate".to_string(), format!("{}M", limit)]);
                info!("Download speed limit: {} MB/s", limit);
            }
        }

        let has_clip_ranges = request
            .clip_ranges
            .as_ref()
            .map(|r| !r.is_empty())
            .unwrap_or(false);
        let has_multiple_sections = request
            .clip_ranges
            .as_ref()
            .map(|r| r.len() > 1)
            .unwrap_or(false);

        if has_multiple_sections {
            if let Some(pos) = args.iter().position(|a| a == "-o") {
                if pos + 1 < args.len() {
                    let original_template = args[pos + 1].clone();
                    let new_template = if original_template.contains(".%(ext)s") {
                        original_template
                            .replace(".%(ext)s", "_%(section_start)s-%(section_end)s.%(ext)s")
                    } else if let Some(dot_pos) = original_template.rfind('.') {
                        let (name, ext) = original_template.split_at(dot_pos);
                        format!("{}_%(section_start)s-%(section_end)s{}", name, ext)
                    } else {
                        format!("{}_%(section_start)s-%(section_end)s", original_template)
                    };
                    args[pos + 1] = new_template.clone();
                    info!("Multi-section output template: {}", new_template);
                }
            }
        }

        if let Some(ranges) = request.clip_ranges.as_ref() {
            for range in ranges.iter() {
                let start_h = (range.start / 3600.0).floor() as u32;
                let start_m = ((range.start % 3600.0) / 60.0).floor() as u32;
                let start_s = range.start % 60.0;
                let end_h = (range.end / 3600.0).floor() as u32;
                let end_m = ((range.end % 3600.0) / 60.0).floor() as u32;
                let end_s = range.end % 60.0;

                let section = format!(
                    "*{:02}:{:02}:{:06.3}-{:02}:{:02}:{:06.3}",
                    start_h, start_m, start_s, end_h, end_m, end_s
                );
                args.extend(["--download-sections".to_string(), section]);
            }
            if has_clip_ranges {
                info!("Clip ranges: {} section(s)", ranges.len());
            }
        }

        // Cutting on non-keyframes can cause short A/V desync right after the cut.
        // This makes ffmpeg ensure keyframes at the cut points for more stable clips.
        if has_clip_ranges {
            args.push("--force-keyframes-at-cuts".to_string());
            info!("Force keyframes at cut points");
        }

        if let Some(fragments) = request.concurrent_fragments {
            if fragments > 1 {
                args.extend([
                    "--concurrent-fragments".to_string(),
                    fragments.to_string(),
                ]);
                info!("Using {} concurrent fragments", fragments);
            }
        }

        if let Some(r) = request.retries {
            args.extend(["--retries".to_string(), r.to_string()]);
        }

        if let Some(fr) = request.fragment_retries {
            args.extend(["--fragment-retries".to_string(), fr.to_string()]);
        }

        if request.keep_original.unwrap_or(false) {
            args.push("--keep-video".to_string());
            info!("Keeping original file after processing");
        }

        if request.restrict_filenames.unwrap_or(false) {
            args.push("--restrict-filenames".to_string());
            info!("Using restricted filenames (ASCII only)");
        }

        if request.windows_filenames.unwrap_or(false) {
            args.push("--windows-filenames".to_string());
            info!("Using Windows-safe filenames");
        }

        if let Some(ref custom_args) = request.download_custom_args {
            if !custom_args.trim().is_empty() {
                for arg in custom_args.split_whitespace() {
                    args.push(arg.to_string());
                }
                info!("Using custom download args: {}", custom_args);
            }
        }

        if let Some(ref pp_args) = request.post_process_custom_args {
            if !pp_args.trim().is_empty() {
                for arg in pp_args.split_whitespace() {
                    args.push(arg.to_string());
                }
                info!("Using custom post-processing args: {}", pp_args);
            }
        }

        let should_use_aria2 = request.use_aria2.unwrap_or(true) && !has_clip_ranges;
        if should_use_aria2 {
            let aria2_path = deps::get_aria2_path(app)?;
            if aria2_path.exists() {
                let connections = request.aria2_connections.unwrap_or(8).min(16).max(1);
                let splits = request.aria2_splits.unwrap_or(8).min(16).max(1);
                let min_split = request.aria2_min_split_size.as_deref().unwrap_or("1M");
                let disable_ipv6 = request.aria2_disable_ipv6.unwrap_or(true);
                let custom_args = request.aria2_custom_args.as_deref().unwrap_or("");

                info!(
                    "Using aria2 as external downloader: {:?} (connections: {}, splits: {}, min-split: {}, disable-ipv6: {})",
                    aria2_path, connections, splits, min_split, disable_ipv6
                );

                let mut aria2_args = format!(
                    "aria2c:-x {} -s {} -k {} --file-allocation=none --retry-wait=2 --min-tls-version=TLSv1.2 --enable-color=false",
                    connections, splits, min_split
                );

                if disable_ipv6 {
                    aria2_args.push_str(" --disable-ipv6=true");
                }

                if !custom_args.is_empty() {
                    aria2_args.push(' ');
                    aria2_args.push_str(custom_args);
                    info!("Using custom aria2 args: {}", custom_args);
                }

                args.extend([
                    "--downloader".to_string(),
                    aria2_path.to_string_lossy().to_string(),
                    "--downloader-args".to_string(),
                    aria2_args,
                ]);
            } else {
                info!("aria2 requested but not installed, using default downloader");
            }
        } else if has_clip_ranges {
            info!("aria2 disabled for sectioned download, using yt-dlp's native downloader");
        } else {
            info!("aria2 disabled by user, using yt-dlp's native downloader");
        }

        let ytdlp_url = normalize_url_for_ytdlp(&request.url);
        apply_site_headers(&ytdlp_url, &mut args);
        args.push(ytdlp_url);

        let env_map: std::collections::HashMap<String, String> =
            config.env_vars.into_iter().collect();
        let env_opt = if env_map.is_empty() { None } else { Some(env_map) };

        let mut cleanup_files: Vec<String> = vec![];
        if use_custom_cookies {
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                cleanup_files.push(
                    cache_dir
                        .join("custom_cookies.txt")
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }

        job_engine::spawn_process_job(
            window,
            registry,
            format!("download_video: {}", request.url),
            config.ytdlp_path,
            args,
            None,
            env_opt,
            cleanup_files,
            Some(downloads_dir.to_string_lossy().to_string()),
        )
        .await
    }
}
