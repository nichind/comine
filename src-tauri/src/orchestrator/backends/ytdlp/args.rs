use crate::orchestrator::backends::{DownloadRequest, ResolveSettings};
use crate::orchestrator::types::*;

use super::common::{maybe_write_cookie_tempfile, TempCookieFile};

pub struct PreparedCookieProxy {
    pub proxy_url: Option<String>,
    pub cookie_arg: Option<String>,
    pub cookies_from_browser: Option<String>,
    /// Must be kept alive until the command completes if cookies were written to a temp file.
    pub _cookie_guard: Option<TempCookieFile>,
}

impl PreparedCookieProxy {
    pub fn from_resolve_settings(settings: &ResolveSettings) -> Self {
        let mut _cookie_guard = None;
        let mut cookie_arg = None;
        if let Some(ref cookies) = settings.custom_cookies {
            if !cookies.is_empty() {
                let (path, guard) = maybe_write_cookie_tempfile(cookies);
                cookie_arg = Some(path);
                _cookie_guard = guard;
            }
        }
        let proxy_url = crate::orchestrator::backends::resolve_effective_proxy(&settings.proxy);
        Self {
            proxy_url,
            cookie_arg,
            cookies_from_browser: settings.cookies_from_browser.clone(),
            _cookie_guard,
        }
    }

    pub fn from_download_options(opts: &DownloadOptions) -> Self {
        let mut _cookie_guard = None;
        let cookie_arg = opts.custom_cookies.as_ref().and_then(|cookies| {
            if cookies.is_empty() {
                None
            } else {
                let (cookie_arg, guard) = maybe_write_cookie_tempfile(cookies);
                _cookie_guard = guard;
                Some(cookie_arg)
            }
        });
        let proxy_url = crate::orchestrator::backends::resolve_effective_proxy(&opts.proxy);
        Self {
            proxy_url,
            cookie_arg,
            cookies_from_browser: opts.cookies_from_browser.clone(),
            _cookie_guard,
        }
    }
}

pub fn normalize_url_for_ytdlp(url: &str) -> String {
    use url::Url;

    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };

    let host = parsed.host_str().unwrap_or("").to_lowercase();

    // Vimeo unlisted links: prefer the player endpoint with the `h=` token.
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

            if host == "vimeo.com"
                && segments.len() >= 2
                && segments[0].chars().all(|c| c.is_ascii_digit())
                && !segments[1].is_empty()
                && segments[1].chars().all(|c| c.is_ascii_hexdigit())
            {
                let id = &segments[0];
                let hash = &segments[1];
                let Ok(mut player) = Url::parse(&format!("https://player.vimeo.com/video/{}", id))
                else {
                    return url.to_string();
                };

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

fn prefer_m3u8_format(original: &str) -> String {
    if original.contains("m3u8") || original.chars().all(|c| c.is_ascii_digit() || c == '+') {
        return original.to_string();
    }

    let is_audio_only = original.contains("bestaudio") && !original.contains("bestvideo");

    if is_audio_only {
        format!(
            "bestaudio[protocol=m3u8_native]/best[protocol=m3u8_native]/{}",
            original
        )
    } else {
        format!("best[protocol=m3u8_native]/{}", original)
    }
}

pub struct YtDlpArgsBuilder {
    pub url: String,
    pub opts: Vec<Vec<String>>,
}

impl YtDlpArgsBuilder {
    pub fn new(url: &str) -> Self {
        Self {
            url: normalize_url_for_ytdlp(url),
            opts: Vec::with_capacity(64),
        }
    }

    pub fn add_arg(mut self, arg: &str) -> Self {
        self.opts.push(vec![arg.to_string()]);
        self
    }

    pub fn add_pair(mut self, key: &str, val: String) -> Self {
        self.opts.push(vec![key.to_string(), val]);
        self
    }

    pub fn with_proxy(mut self, proxy_url: Option<String>) -> Self {
        if let Some(p) = proxy_url {
            if !p.is_empty() {
                self = self.add_pair("--proxy", p);
            }
        }
        self
    }

    pub fn with_cookies(
        mut self,
        cookies_from_browser: Option<String>,
        custom_cookies: Option<String>,
    ) -> Self {
        if let Some(browser) = cookies_from_browser {
            if !browser.is_empty() && browser != "custom" {
                self = self.add_pair("--cookies-from-browser", browser);
            }
        }

        if let Some(cookies) = custom_cookies {
            if !cookies.is_empty() {
                self = self.add_pair("--cookies", cookies);
            }
        }
        self
    }

    pub fn with_player_client(mut self, client: Option<String>) -> Self {
        if let Some(c) = client {
            if !c.is_empty() {
                self = self.add_pair("--extractor-args", format!("youtube:player_client={}", c));
            }
        }
        self
    }

    pub fn with_cache_dir(mut self, cache_dir: Option<String>) -> Self {
        if let Some(dir) = cache_dir {
            if !dir.is_empty() {
                self = self.add_pair("--cache-dir", dir);
            }
        }
        self
    }

    pub fn with_js_runtimes(mut self, runtimes: Vec<(String, String)>) -> Self {
        for (name, path) in runtimes {
            if !path.is_empty() {
                self = self.add_pair("--js-runtimes", format!("{}:{}", name, path));
            }
        }
        self
    }

    pub fn apply_common_headers(mut self) -> Self {
        self = self.add_pair("--user-agent", constants::USER_AGENT.to_string());

        let is_bilibili = self.url.contains("bilibili.com")
            || self.url.contains("b23.tv")
            || self.url.contains("bilivideo.com");
        if is_bilibili {
            self = self.add_pair("--referer", "https://www.bilibili.com/".to_string());
            self = self.add_pair(
                "--add-header",
                "Origin:https://www.bilibili.com".to_string(),
            );
            self = self.add_pair(
                "--add-header",
                "Accept-Language:zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7".to_string(),
            );
        }
        self
    }

    pub fn build_resolve(mut self, settings: &ResolveSettings) -> Vec<Vec<String>> {
        self = self.add_pair("--encoding", "utf-8".to_string());

        self = self
            .add_arg("--dump-json")
            .add_arg("--no-download")
            .add_arg("--no-warnings")
            .add_arg("--ignore-errors")
            .add_arg("--geo-bypass")
            .add_pair("--socket-timeout", "15".to_string())
            .add_pair("--extractor-retries", "2".to_string());

        if settings.flat_playlist {
            self = self.add_arg("--flat-playlist");

            if let Some(page_size) = settings.page_size {
                let start: u32 = settings
                    .cursor
                    .as_ref()
                    .and_then(|c| c.parse().ok())
                    .unwrap_or(1);
                let end = start.saturating_add(page_size).saturating_sub(1);
                self = self.add_pair("--playlist-items", format!("{}:{}", start, end));
            }
        } else {
            self = self.add_arg("--no-playlist");
        }

        self = self
            .with_player_client(settings.youtube_player_client.clone())
            .apply_common_headers();

        self.opts
    }

    pub fn build_download(
        mut self,
        req: &DownloadRequest,
        effective_speed_limit: Option<u64>,
        ffmpeg_location: Option<String>,
    ) -> Vec<Vec<String>> {
        self.opts
            .push(vec!["--encoding".to_string(), "utf-8".to_string()]);

        if let Some(loc) = ffmpeg_location {
            if !loc.is_empty() {
                self = self.add_pair("--ffmpeg-location", loc);
            }
        }

        let base_template = req
            .output
            .filename_template
            .as_deref()
            .unwrap_or("%(title)s.%(ext)s");

        let has_multiple_sections = req
            .options
            .clip_ranges
            .as_ref()
            .map(|r| r.len() > 1)
            .unwrap_or(false);

        let template = if has_multiple_sections {
            if let Some(dot_pos) = base_template.rfind('.') {
                format!(
                    "{}_%(section_start)s-%(section_end)s{}",
                    &base_template[..dot_pos],
                    &base_template[dot_pos..]
                )
            } else {
                format!("{}_%(section_start)s-%(section_end)s", base_template)
            }
        } else {
            base_template.to_string()
        };

        let output_path = format!("{}/{}", req.output.directory, template);
        self = self.add_pair("-o", output_path);

        let has_sections = req
            .options
            .clip_ranges
            .as_ref()
            .is_some_and(|r| !r.is_empty());

        if req.quality.audio_only {
            self = self.add_arg("-x");
            let format = if has_sections {
                prefer_m3u8_format(&req.quality.format)
            } else {
                req.quality.format.clone()
            };
            self = self.add_pair("-f", format);
            if let Some(ref fmt) = req.quality.audio_format {
                self = self.add_pair("--audio-format", fmt.clone());
            }
        } else {
            let format = if has_sections {
                prefer_m3u8_format(&req.quality.format)
            } else {
                req.quality.format.clone()
            };
            self = self.add_pair("-f", format);
            if let Some(height) = req.quality.max_height {
                self = self.add_pair("-S", format!("res:{}", height));
            }
            // Force MP4 container for video downloads so the built-in HTML5 player can play
            // the output. yt-dlp defaults to MKV when merging separate video+audio streams.
            // --merge-output-format handles the merge path; --remux-video is a safety net for
            // cases where yt-dlp downloads a single stream that is already muxed but not MP4.
            self = self.add_pair("--merge-output-format", "mp4".to_string());
            self = self.add_pair("--remux-video", "mp4".to_string());
        }

        if req.options.embed_metadata {
            self = self.add_arg("--embed-metadata");
        }
        if req.options.embed_subtitles {
            self = self.add_arg("--embed-subs");
            if let Some(ref langs) = req.options.subtitle_langs {
                self = self.add_pair("--sub-langs", langs.clone());
            }
        }

        if let Some(ref categories) = req.options.sponsorblock_remove {
            self = self.add_pair("--sponsorblock-remove", categories.clone());
        }

        if let Some(ref ranges) = req.options.clip_ranges {
            tracing::info!(target: "ytdlp", "Processing {} clip ranges: {:?}", ranges.len(), ranges);
            for range in ranges {
                self = self.add_pair(
                    "--download-sections",
                    format!("*{:.2}-{:.2}", range.start, range.end),
                );
            }
            if req.options.force_keyframes_at_cuts {
                self = self.add_arg("--force-keyframes-at-cuts");
            }
        }

        if let Some(limit) = effective_speed_limit {
            if limit > 0 {
                self = self.add_pair("--limit-rate", format!("{}K", limit / 1024));
            }
        }

        if let Some(retries) = req.options.max_retries {
            self = self.add_pair("--retries", retries.to_string());
        }

        if req.options.use_aria2 {
            #[cfg(target_os = "android")]
            let aria2_bin = "libaria2c.so";
            #[cfg(desktop)]
            let aria2_bin = "aria2c";
            #[cfg(target_os = "ios")]
            let aria2_bin = "aria2c";

            self = self.add_pair("--external-downloader", aria2_bin.to_string());
            let conns = req.options.aria2_connections.unwrap_or(8);
            let splits = req.options.aria2_splits.unwrap_or(8);
            let min_split = req.options.aria2_min_split_size.as_deref().unwrap_or("1M");
            let disable_ipv6 = req.options.aria2_disable_ipv6.unwrap_or(true);
            let custom_args = req
                .options
                .aria2_custom_args
                .as_deref()
                .unwrap_or("")
                .trim();

            let mut aria2_args = format!(
                "{}:-x {} -s {} -k {} --disable-ipv6={} --show-console-readout=true --summary-interval=1",
                aria2_bin, conns, splits, min_split, disable_ipv6
            );
            if !custom_args.is_empty() {
                aria2_args.push(' ');
                aria2_args.push_str(custom_args);
            }
            self = self.add_pair("--external-downloader-args", aria2_args);
        }

        self = self
            .add_arg("--continue")
            .add_arg("--progress")
            .add_arg("--newline")
            .add_pair(
                "--progress-template",
                r#"download:__COMINE_PROGRESS__{downloaded:%(progress.downloaded_bytes)s,total:%(progress.total_bytes)s,total_estimate:%(progress.total_bytes_estimate)s,speed:%(progress.speed)s,eta:%(progress.eta)s,filename:%(progress.filename)s}__COMINE_PROGRESS__"#.to_string(),
            );

        let markers = [
            "pre_process:>>>TITLE:%(title)s",
            "pre_process:>>>THUMBNAIL:%(thumbnail)s",
            "pre_process:>>>UPLOADER:%(uploader)s",
            "pre_process:>>>CHANNEL:%(channel)s",
            "pre_process:>>>CHANNEL_URL:%(channel_url)s",
            "pre_process:>>>DURATION:%(duration)s",
            "pre_process:>>>EXTRACTOR:%(extractor)s",
            "pre_process:>>>WEBPAGE_URL:%(webpage_url)s",
            "pre_process:>>>PLAYLIST_COUNT:%(playlist_count)s",
            "after_move:>>>FILEPATH:%(filepath)s",
        ];

        for m in markers {
            self = self.add_pair("--print", m.to_string());
        }

        self = self
            .with_player_client(req.options.youtube_player_client.clone())
            .apply_common_headers();

        self.opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn opts_contain(opts: &[Vec<String>], flag: &str) -> bool {
        opts.iter().any(|group| group.iter().any(|a| a == flag))
    }

    fn opts_get_pair(opts: &[Vec<String>], key: &str) -> Option<String> {
        opts.iter().find_map(|group| {
            if group.len() == 2 && group[0] == key {
                Some(group[1].clone())
            } else {
                None
            }
        })
    }

    fn opts_get_all_pairs(opts: &[Vec<String>], key: &str) -> Vec<String> {
        opts.iter()
            .filter_map(|group| {
                if group.len() == 2 && group[0] == key {
                    Some(group[1].clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn test_request(url: &str) -> DownloadRequest {
        DownloadRequest {
            url: url.to_string(),
            backend: None,
            id: None,
            quality: QualitySettings::default(),
            output: OutputSettings {
                directory: "/tmp/downloads".to_string(),
                filename_template: None,
                filename: None,
            },
            options: DownloadOptions::default(),
            post_process: Vec::new(),
        }
    }

    fn test_resolve_settings() -> ResolveSettings {
        ResolveSettings {
            cookies_from_browser: None,
            custom_cookies: None,
            proxy: None,
            youtube_player_client: None,
            flat_playlist: false,
            page_size: None,
            cursor: None,
        }
    }

    #[test]
    fn test_normalize_url_passthrough() {
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(normalize_url_for_ytdlp(url), url);
    }

    #[test]
    fn test_normalize_url_non_url() {
        let url = "not a url";
        assert_eq!(normalize_url_for_ytdlp(url), url);
    }

    #[test]
    fn test_normalize_url_vimeo_with_h_param() {
        let url = "https://vimeo.com/123456?h=abc123";
        assert_eq!(normalize_url_for_ytdlp(url), url);
    }

    #[test]
    fn test_normalize_url_vimeo_unlisted_hash_in_path() {
        let url = "https://vimeo.com/123456/abcdef0123456789";
        let result = normalize_url_for_ytdlp(url);
        assert!(result.contains("player.vimeo.com/video/123456"));
        assert!(result.contains("h=abcdef0123456789"));
    }

    #[test]
    fn test_normalize_url_vimeo_player_hash_in_path() {
        let url = "https://player.vimeo.com/video/123456/abcdef0123456789";
        let result = normalize_url_for_ytdlp(url);
        assert!(result.contains("player.vimeo.com/video/123456"));
        assert!(result.contains("h=abcdef0123456789"));
        let parsed = url::Url::parse(&result).unwrap();
        assert_eq!(parsed.path(), "/video/123456");
    }

    #[test]
    fn test_normalize_url_vimeo_regular_video() {
        let url = "https://vimeo.com/123456";
        assert_eq!(normalize_url_for_ytdlp(url), url);
    }

    #[test]
    fn test_normalize_url_non_vimeo_subdomain() {
        let url = "https://example.com/123456/abcdef";
        assert_eq!(normalize_url_for_ytdlp(url), url);
    }

    #[test]
    fn test_build_resolve_basic() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test");
        let settings = test_resolve_settings();
        let opts = builder.build_resolve(&settings);

        assert!(opts_contain(&opts, "--dump-json"));
        assert!(opts_contain(&opts, "--no-download"));
        assert!(opts_contain(&opts, "--no-warnings"));
        assert!(opts_contain(&opts, "--ignore-errors"));
        assert!(opts_contain(&opts, "--no-playlist"));
        assert_eq!(
            opts_get_pair(&opts, "--encoding"),
            Some("utf-8".to_string())
        );
    }

    #[test]
    fn test_build_resolve_flat_playlist() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/playlist?list=PLtest");
        let settings = ResolveSettings {
            flat_playlist: true,
            ..test_resolve_settings()
        };
        let opts = builder.build_resolve(&settings);

        assert!(opts_contain(&opts, "--flat-playlist"));
        assert!(!opts_contain(&opts, "--no-playlist"));
    }

    #[test]
    fn test_build_resolve_playlist_pagination() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/playlist?list=PLtest");
        let settings = ResolveSettings {
            flat_playlist: true,
            page_size: Some(20),
            cursor: Some("5".to_string()),
            ..test_resolve_settings()
        };
        let opts = builder.build_resolve(&settings);

        assert_eq!(
            opts_get_pair(&opts, "--playlist-items"),
            Some("5:24".to_string())
        );
    }

    #[test]
    fn test_build_resolve_playlist_pagination_no_cursor() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/playlist?list=PLtest");
        let settings = ResolveSettings {
            flat_playlist: true,
            page_size: Some(10),
            cursor: None,
            ..test_resolve_settings()
        };
        let opts = builder.build_resolve(&settings);

        assert_eq!(
            opts_get_pair(&opts, "--playlist-items"),
            Some("1:10".to_string())
        );
    }

    #[test]
    fn test_build_resolve_player_client() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test");
        let settings = ResolveSettings {
            youtube_player_client: Some("android_vr".to_string()),
            ..test_resolve_settings()
        };
        let opts = builder.build_resolve(&settings);

        assert_eq!(
            opts_get_pair(&opts, "--extractor-args"),
            Some("youtube:player_client=android_vr".to_string())
        );
    }

    #[test]
    fn test_build_resolve_has_user_agent() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test");
        let settings = test_resolve_settings();
        let opts = builder.build_resolve(&settings);

        let ua = opts_get_pair(&opts, "--user-agent");
        assert!(ua.is_some());
        assert!(ua.unwrap().contains("Mozilla"));
    }

    #[test]
    fn test_build_download_basic() {
        let req = test_request("https://youtube.com/watch?v=test");
        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let output = opts_get_pair(&opts, "-o");
        assert!(output.is_some());
        assert!(output.as_ref().unwrap().starts_with("/tmp/downloads/"));
        assert!(output.unwrap().contains("%(title)s.%(ext)s"));

        assert_eq!(
            opts_get_pair(&opts, "-f"),
            Some("bestvideo+bestaudio/best".to_string())
        );

        assert!(opts_contain(&opts, "--continue"));
        assert!(opts_contain(&opts, "--progress"));
        assert!(opts_contain(&opts, "--newline"));
        let progress_template = opts_get_pair(&opts, "--progress-template");
        assert!(progress_template.is_some());
        assert!(progress_template.unwrap().contains("__COMINE_PROGRESS__"));

        let print_args = opts_get_all_pairs(&opts, "--print");
        assert!(print_args.iter().any(|a| a.contains(">>>TITLE:")));
        assert!(print_args.iter().any(|a| a.contains(">>>FILEPATH:")));
        assert!(print_args.iter().any(|a| a.contains(">>>THUMBNAIL:")));
    }

    #[test]
    fn test_build_download_audio_only() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.quality.audio_only = true;
        req.quality.format = "bestaudio".to_string();
        req.quality.audio_format = Some("mp3".to_string());

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert!(opts_contain(&opts, "-x")); // extract audio
        assert_eq!(opts_get_pair(&opts, "-f"), Some("bestaudio".to_string()));
        assert_eq!(
            opts_get_pair(&opts, "--audio-format"),
            Some("mp3".to_string())
        );
    }

    #[test]
    fn test_build_download_with_max_height() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.quality.max_height = Some(720);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert_eq!(opts_get_pair(&opts, "-S"), Some("res:720".to_string()));
    }

    #[test]
    fn test_build_download_embed_metadata() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.embed_metadata = true;

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert!(opts_contain(&opts, "--embed-metadata"));
    }

    #[test]
    fn test_build_download_embed_subtitles() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.embed_subtitles = true;
        req.options.subtitle_langs = Some("en,ja".to_string());

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert!(opts_contain(&opts, "--embed-subs"));
        assert_eq!(
            opts_get_pair(&opts, "--sub-langs"),
            Some("en,ja".to_string())
        );
    }

    #[test]
    fn test_build_download_sponsorblock() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.sponsorblock_remove = Some("sponsor,intro".to_string());

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert_eq!(
            opts_get_pair(&opts, "--sponsorblock-remove"),
            Some("sponsor,intro".to_string())
        );
    }

    #[test]
    fn test_build_download_clip_ranges() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.clip_ranges = Some(vec![
            ClipRange {
                id: "1".to_string(),
                start: 10.0,
                end: 30.0,
            },
            ClipRange {
                id: "2".to_string(),
                start: 60.0,
                end: 90.0,
            },
        ]);
        req.options.force_keyframes_at_cuts = true;

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let sections = opts_get_all_pairs(&opts, "--download-sections");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0], "*10.00-30.00");
        assert_eq!(sections[1], "*60.00-90.00");
        assert!(opts_contain(&opts, "--force-keyframes-at-cuts"));

        // With sections, format should prefer m3u8
        let format = opts_get_pair(&opts, "-f").unwrap();
        assert!(format.starts_with("best[protocol=m3u8_native]/"));
    }

    #[test]
    fn test_build_download_sections_m3u8_preference() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.clip_ranges = Some(vec![ClipRange {
            id: "1".to_string(),
            start: 10.0,
            end: 30.0,
        }]);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let format = opts_get_pair(&opts, "-f").unwrap();
        assert_eq!(
            format,
            "best[protocol=m3u8_native]/bestvideo+bestaudio/best"
        );
    }

    #[test]
    fn test_build_download_sections_audio_only_m3u8() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.quality.audio_only = true;
        req.quality.format = "bestaudio".to_string();
        req.options.clip_ranges = Some(vec![ClipRange {
            id: "1".to_string(),
            start: 10.0,
            end: 30.0,
        }]);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let format = opts_get_pair(&opts, "-f").unwrap();
        assert_eq!(
            format,
            "bestaudio[protocol=m3u8_native]/best[protocol=m3u8_native]/bestaudio"
        );
    }

    #[test]
    fn test_build_download_no_sections_no_m3u8() {
        let req = test_request("https://youtube.com/watch?v=test");
        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let format = opts_get_pair(&opts, "-f").unwrap();
        assert_eq!(format, "bestvideo+bestaudio/best");
    }

    #[test]
    fn test_build_download_multiple_sections_template() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.clip_ranges = Some(vec![
            ClipRange {
                id: "1".to_string(),
                start: 0.0,
                end: 10.0,
            },
            ClipRange {
                id: "2".to_string(),
                start: 20.0,
                end: 30.0,
            },
        ]);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let output = opts_get_pair(&opts, "-o").unwrap();
        assert!(output.contains("%(section_start)s"));
        assert!(output.contains("%(section_end)s"));
    }

    #[test]
    fn test_build_download_single_section_no_section_template() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.clip_ranges = Some(vec![ClipRange {
            id: "1".to_string(),
            start: 0.0,
            end: 10.0,
        }]);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let output = opts_get_pair(&opts, "-o").unwrap();
        assert!(!output.contains("%(section_start)s"));
    }

    #[test]
    fn test_build_download_speed_limit() {
        let req = test_request("https://youtube.com/watch?v=test");
        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, Some(1_048_576), None);

        assert_eq!(
            opts_get_pair(&opts, "--limit-rate"),
            Some("1024K".to_string())
        );
    }

    #[test]
    fn test_build_download_ffmpeg_location() {
        let req = test_request("https://youtube.com/watch?v=test");
        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, Some("/usr/bin".to_string()));

        assert_eq!(
            opts_get_pair(&opts, "--ffmpeg-location"),
            Some("/usr/bin".to_string())
        );
    }

    #[test]
    fn test_build_download_aria2_external() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.use_aria2 = true;
        req.options.aria2_connections = Some(16);
        req.options.aria2_splits = Some(16);
        req.options.aria2_min_split_size = Some("2M".to_string());
        req.options.aria2_disable_ipv6 = Some(true);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert_eq!(
            opts_get_pair(&opts, "--external-downloader"),
            Some("aria2c".to_string())
        );
        let aria2_args = opts_get_pair(&opts, "--external-downloader-args").unwrap();
        assert!(aria2_args.starts_with("aria2c:"));
        assert!(aria2_args.contains("-x 16"));
        assert!(aria2_args.contains("-s 16"));
        assert!(aria2_args.contains("-k 2M"));
        assert!(aria2_args.contains("--disable-ipv6=true"));
    }

    #[test]
    fn test_build_download_aria2_custom_args() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.use_aria2 = true;
        req.options.aria2_custom_args = Some("--timeout=60".to_string());

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let aria2_args = opts_get_pair(&opts, "--external-downloader-args").unwrap();
        assert!(aria2_args.contains("--timeout=60"));
    }

    #[test]
    fn test_build_download_max_retries() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.options.max_retries = Some(5);

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert_eq!(opts_get_pair(&opts, "--retries"), Some("5".to_string()));
    }

    #[test]
    fn test_build_download_bilibili_headers() {
        let req = test_request("https://www.bilibili.com/video/BV1xx411c7mD");
        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        assert_eq!(
            opts_get_pair(&opts, "--referer"),
            Some("https://www.bilibili.com/".to_string())
        );
        let add_headers = opts_get_all_pairs(&opts, "--add-header");
        assert!(add_headers
            .iter()
            .any(|h| h.contains("Origin:https://www.bilibili.com")));
    }

    #[test]
    fn test_build_download_custom_filename_template() {
        let mut req = test_request("https://youtube.com/watch?v=test");
        req.output.filename_template = Some("%(uploader)s - %(title)s.%(ext)s".to_string());

        let builder = YtDlpArgsBuilder::new(&req.url);
        let opts = builder.build_download(&req, None, None);

        let output = opts_get_pair(&opts, "-o").unwrap();
        assert!(output.contains("%(uploader)s - %(title)s.%(ext)s"));
    }

    #[test]
    fn test_builder_with_proxy() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test")
            .with_proxy(Some("http://proxy:8080".to_string()));

        assert_eq!(
            opts_get_pair(&builder.opts, "--proxy"),
            Some("http://proxy:8080".to_string())
        );
    }

    #[test]
    fn test_builder_with_proxy_empty() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test")
            .with_proxy(Some("".to_string()));

        assert!(opts_get_pair(&builder.opts, "--proxy").is_none());
    }

    #[test]
    fn test_builder_with_proxy_none() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test").with_proxy(None);

        assert!(opts_get_pair(&builder.opts, "--proxy").is_none());
    }

    #[test]
    fn test_builder_with_cookies_browser() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test")
            .with_cookies(Some("chrome".to_string()), None);

        assert_eq!(
            opts_get_pair(&builder.opts, "--cookies-from-browser"),
            Some("chrome".to_string())
        );
    }

    #[test]
    fn test_builder_with_cookies_custom_browser_ignored() {
        // "custom" browser string means user is using manual cookie file — skip --cookies-from-browser
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test")
            .with_cookies(Some("custom".to_string()), None);

        assert!(opts_get_pair(&builder.opts, "--cookies-from-browser").is_none());
    }

    #[test]
    fn test_builder_with_cookies_file() {
        let builder = YtDlpArgsBuilder::new("https://youtube.com/watch?v=test")
            .with_cookies(None, Some("/tmp/cookies.txt".to_string()));

        assert_eq!(
            opts_get_pair(&builder.opts, "--cookies"),
            Some("/tmp/cookies.txt".to_string())
        );
    }

    #[test]
    fn test_builder_with_js_runtimes() {
        let builder =
            YtDlpArgsBuilder::new("https://youtube.com/watch?v=test").with_js_runtimes(vec![
                ("deno".to_string(), "/usr/local/bin/deno".to_string()),
                ("quickjs".to_string(), "/usr/local/bin/qjs".to_string()),
            ]);

        let runtimes = opts_get_all_pairs(&builder.opts, "--js-runtimes");
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0], "deno:/usr/local/bin/deno");
        assert_eq!(runtimes[1], "quickjs:/usr/local/bin/qjs");
    }

    #[test]
    fn test_builder_with_js_runtimes_empty() {
        let builder =
            YtDlpArgsBuilder::new("https://youtube.com/watch?v=test").with_js_runtimes(vec![]);

        assert!(opts_get_all_pairs(&builder.opts, "--js-runtimes").is_empty());
    }
}
