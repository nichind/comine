use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use crate::orchestrator::backends::{
    graceful_shutdown, resolve_effective_proxy, Backend, BackendCapabilities, SpawnContext,
};
use crate::orchestrator::types::*;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, info, warn};

const GALLERY_DOMAINS: &[&str] = &[
    "pixiv.net",
    "danbooru.donmai.us",
    "gelbooru.com",
    "safebooru.org",
    "konachan.com",
    "yande.re",
    "sankaku",
    "rule34.xxx",
    "e621.net",
    "e926.net",
    "zerochan.net",
    "deviantart.com",
    "artstation.com",
    "hentai-foundry.com",
    "furaffinity.net",
    "inkbunny.net",
    "newgrounds.com",
    "behance.net",
    "imgur.com",
    "flickr.com",
    "catbox.moe",
    "imgbox.com",
    "postimg.cc",
    "500px.com",
    "nhentai",
    "hitomi.la",
    "exhentai.org",
    "e-hentai.org",
    "luscious.net",
    "tsumino.com",
    "pururin.to",
    "mangadex.org",
    "mangapark.to",
    "mangasee",
    "dynastyscans.com",
    "readcomiconline",
    "webtoons.com",
    "tapas.io",
    "kemono.su",
    "kemono.cr",
    "coomer.su",
    "fanbox.cc",
    "patreon.com",
    "subscribestar.adult",
    "fantia.jp",
    "boosty.to",
    "gumroad.com",
    "ko-fi.com",
    "tumblr.com",
    "pillowfort.social",
    "cohost.org",
    "bsky.app",
    "weibo.com",
    "nitter.",
    "pinterest.",
];

const VIDEO_PRIMARY_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "twitch.tv",
    "vimeo.com",
    "dailymotion.com",
    "tiktok.com",
    "bilibili.com",
    "nicovideo.jp",
    "soundcloud.com",
    "bandcamp.com",
    "fb.watch",
];

const SHARED_DOMAINS: &[&str] = &["twitter.com", "x.com", "instagram.com", "reddit.com"];

pub struct GalleryDlBackend {
    binary_path: PathBuf,
}

impl GalleryDlBackend {
    pub fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    async fn resolve_impl(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        let mut cmd = crate::utils::new_command(&self.binary_path);
        cmd.arg("--dump-json");
        cmd.arg("--no-download");

        if let Some(proxy_url) = resolve_effective_proxy(&settings.proxy) {
            cmd.arg("--proxy");
            cmd.arg(&proxy_url);
        }

        if let Some(ref cookies_browser) = settings.cookies_from_browser {
            cmd.arg("--cookies-from-browser");
            cmd.arg(cookies_browser);
        }

        cmd.arg("--");
        cmd.arg(url);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| BackendError::ProcessError(format!("Failed to run gallery-dl: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(parse_gallery_dl_error(&stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_gallery_dl_dump_json(&stdout, url)
    }

    async fn spawn_impl(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        info!(target: "gallery-dl", "Starting download job {} for URL: {}", ctx.job.id, ctx.job.request.url);

        let mut cmd = crate::utils::new_command(&self.binary_path);
        let req = &ctx.job.request;
        let opts = &req.options;

        cmd.arg("-d");
        cmd.arg(&req.output.directory);

        if let Some(ref tmpl) = req.output.filename_template {
            cmd.arg("-f");
            cmd.arg(tmpl);
        }

        if let Some(proxy_url) = resolve_effective_proxy(&opts.proxy) {
            cmd.arg("--proxy");
            cmd.arg(&proxy_url);
        }

        if let Some(ref cookies_browser) = opts.cookies_from_browser {
            cmd.arg("--cookies-from-browser");
            cmd.arg(cookies_browser);
        }

        if let Some(limit) = ctx.effective_speed_limit {
            if limit > 0 {
                cmd.arg("--rate-limit");
                cmd.arg(format!("{}K", limit / 1024));
            }
        }

        if let Some(retries) = opts.max_retries {
            cmd.arg("--retries");
            cmd.arg(retries.to_string());
        }

        cmd.arg("--");
        cmd.arg(&req.url);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            BackendError::ProcessError(format!("Failed to spawn gallery-dl: {}", e))
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::ProcessError("Failed to capture stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| BackendError::ProcessError("Failed to capture stderr".to_string()))?;

        let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(
            VecDeque::with_capacity(constants::STDERR_TAIL_SIZE),
        ));
        let stderr_tail_clone = stderr_tail.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                warn!(target: "gallery-dl", "ERR: {}", line);
                let mut guard = match stderr_tail_clone.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                if guard.len() >= constants::STDERR_TAIL_SIZE {
                    guard.pop_front();
                }
                guard.push_back(line);
            }
        });

        let mut reader = BufReader::new(stdout).lines();
        let mut downloaded_count: u64 = 0;
        let mut last_output_path: Option<String> = None;
        let mut output_file_count: u32 = 0;

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    graceful_shutdown(&mut child, "gallery-dl").await;
                    return Err(BackendError::Cancelled);
                }
                line = reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            let line = line.trim().to_string();
                            if line.is_empty() {
                                continue;
                            }

                            debug!(target: "gallery-dl", "OUT: {}", line);

                            if let Some(progress) = parse_gallery_dl_progress(&line, &ctx.job.id, &mut downloaded_count) {
                                let _ = ctx.progress_tx.send(progress);
                            }

                            if line.starts_with('/') || line.starts_with("\\\\") || (line.len() > 2 && line.as_bytes()[1] == b':') {
                                last_output_path = Some(line.clone());
                                output_file_count += 1;
                            } else if line.starts_with("# ") {
                            } else if !line.starts_with('[') && !line.starts_with('{') {
                                let candidate = PathBuf::from(&req.output.directory).join(&line);
                                if line.contains('.') {
                                    last_output_path = Some(candidate.to_string_lossy().to_string());
                                    output_file_count += 1;
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            warn!(target: "gallery-dl", "Error reading stdout: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        if !status.success() {
            let tail = {
                let guard = match stderr_tail.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                guard.iter().cloned().collect::<Vec<_>>().join("\n")
            };
            let tail_msg = if tail.trim().is_empty() {
                String::new()
            } else {
                format!("\n\ngallery-dl stderr tail:\n{}", tail)
            };

            return Err(BackendError::ProcessError(format!(
                "gallery-dl exited with code {:?}{}",
                status.code(),
                tail_msg
            )));
        }

        let output = if output_file_count > 1 {
            // Multi-file download: return the directory, not the last individual file
            req.output.directory.clone()
        } else {
            last_output_path.unwrap_or_else(|| req.output.directory.clone())
        };
        Ok(output)
    }
}

#[async_trait]
impl Backend for GalleryDlBackend {
    fn name(&self) -> &str {
        "gallery-dl"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            name: "gallery-dl".into(),
            streaming_resolve: false,
            playlists: true, // galleries are effectively playlists of images
            pause_resume: false,
            multi_connection: false,
            format_selection: false,
            subtitles: false,
            speed_limit: true,
            proxy: true,
            cookies: true,
            torrent_magnet: false,
            post_processing: false,
        }
    }

    fn priority(&self, url: &str) -> Priority {
        if VIDEO_PRIMARY_DOMAINS.iter().any(|d| url.contains(d)) {
            return Priority::None;
        }

        if SHARED_DOMAINS.iter().any(|d| url.contains(d)) {
            return Priority::Medium;
        }

        if GALLERY_DOMAINS.iter().any(|d| url.contains(d)) {
            return Priority::High;
        }

        if url.starts_with("http://") || url.starts_with("https://") {
            Priority::Low
        } else {
            Priority::None
        }
    }

    async fn resolve(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        self.resolve_impl(url, settings).await
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        self.spawn_impl(ctx).await
    }
}

fn extract_tags(obj: &serde_json::Map<String, serde_json::Value>, tags: &mut Vec<String>) {
    if let Some(tag_val) = obj.get("tags") {
        match tag_val {
            serde_json::Value::Array(arr) => {
                for t in arr {
                    if let Some(s) = t.as_str() {
                        if !tags.contains(&s.to_string()) {
                            tags.push(s.to_string());
                        }
                    }
                }
            }
            serde_json::Value::String(s) => {
                for t in s.split_whitespace() {
                    if !tags.contains(&t.to_string()) {
                        tags.push(t.to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

fn parse_gallery_dl_dump_json(stdout: &str, url: &str) -> Result<UrlInfo, BackendError> {
    let mut files: Vec<FileEntry> = Vec::new();
    let mut gallery_title: Option<String> = None;
    let mut gallery_artist: Option<String> = None;
    let mut gallery_category: Option<String> = None;
    let mut thumbnail: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();

    // gallery-dl --dump-json outputs a single JSON array of entries:
    //   [[2, {post_metadata}], [3, "url", {file_metadata}], ...]
    //
    // - Type 2 entries: [2, {metadata}] — directory/gallery metadata
    // - Type 3 entries: [3, "url", {metadata}] — individual downloadable files
    //
    // We also support line-delimited JSON where each line is a standalone entry
    // (used by some gallery-dl versions or piped outputs).

    let entries: Vec<serde_json::Value> = {
        let trimmed = stdout.trim();

        if let Ok(serde_json::Value::Array(arr)) =
            serde_json::from_str::<serde_json::Value>(trimmed)
        {
            if arr.iter().all(|v| v.is_array()) {
                let is_top_level = arr.first().is_some_and(|first| {
                    first.as_array().is_some_and(|inner| {
                        inner.first().is_some_and(|f| f.is_u64())
                            && inner.last().is_some_and(|l| l.is_object() || l.is_string())
                    })
                });

                if is_top_level {
                    arr
                } else {
                    vec![serde_json::Value::Array(arr)]
                }
            } else {
                vec![serde_json::Value::Array(arr)]
            }
        } else {
            let mut entries = Vec::new();
            for line in trimmed.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    entries.push(v);
                }
            }
            entries
        }
    };

    let mut file_idx = 0u32;
    for entry in &entries {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };

        let type_code = arr[0].as_u64().unwrap_or(0);

        match type_code {
            2 => {
                let metadata = &arr[1];
                if let Some(obj) = metadata.as_object() {
                    if gallery_title.is_none() {
                        gallery_title = obj
                            .get("title")
                            .or_else(|| obj.get("description"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if gallery_artist.is_none() {
                        gallery_artist = obj
                            .get("artist")
                            .or_else(|| obj.get("author"))
                            .or_else(|| obj.get("uploader"))
                            .or_else(|| obj.get("username"))
                            .and_then(|v| match v {
                                serde_json::Value::String(s) => Some(s.clone()),
                                serde_json::Value::Object(o) => {
                                    o.get("name").and_then(|n| n.as_str()).map(String::from)
                                }
                                _ => None,
                            });
                    }
                    if gallery_category.is_none() {
                        gallery_category = obj
                            .get("category")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if thumbnail.is_none() {
                        thumbnail = obj
                            .get("file")
                            .and_then(|f| f.get("url"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    extract_tags(obj, &mut tags);
                }
            }
            3 => {
                let metadata = if arr.len() >= 3 { &arr[2] } else { &arr[1] };
                if let Some(obj) = metadata.as_object() {
                    let file_url = obj
                        .get("url")
                        .or_else(|| obj.get("file_url"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| arr.get(1).and_then(|v| v.as_str()).map(String::from));

                    let filename = obj
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .map(|f| {
                            if let Some(ext) = obj.get("extension").and_then(|e| e.as_str()) {
                                format!("{}.{}", f, ext)
                            } else {
                                f.to_string()
                            }
                        })
                        .or_else(|| obj.get("name").and_then(|v| v.as_str()).map(String::from));

                    let filesize = obj
                        .get("filesize")
                        .or_else(|| obj.get("size"))
                        .and_then(|v| v.as_u64());

                    let width = obj.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
                    let height = obj.get("height").and_then(|v| v.as_u64()).map(|v| v as u32);

                    let file_thumb = obj
                        .get("thumbnail")
                        .or_else(|| obj.get("preview_url"))
                        .or_else(|| obj.get("sample_url"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    if thumbnail.is_none() {
                        thumbnail = file_thumb.clone().or_else(|| file_url.clone());
                    }

                    files.push(FileEntry {
                        url: file_url,
                        path: None,
                        filename,
                        filesize,
                        mime_type: obj
                            .get("extension")
                            .and_then(|v| v.as_str())
                            .and_then(|ext| {
                                crate::orchestrator::backends::guess_mime_type(&format!(
                                    "x.{}",
                                    ext
                                ))
                            }),
                        thumbnail: file_thumb,
                        index: Some(file_idx),
                        width,
                        height,
                    });
                    file_idx += 1;

                    if gallery_title.is_none() {
                        gallery_title = obj
                            .get("title")
                            .or_else(|| obj.get("description"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if gallery_artist.is_none() {
                        gallery_artist = obj
                            .get("artist")
                            .or_else(|| obj.get("author"))
                            .or_else(|| obj.get("uploader"))
                            .or_else(|| obj.get("username"))
                            .and_then(|v| match v {
                                serde_json::Value::String(s) => Some(s.clone()),
                                serde_json::Value::Object(o) => {
                                    o.get("name").and_then(|n| n.as_str()).map(String::from)
                                }
                                _ => None,
                            });
                    }
                    if gallery_category.is_none() {
                        gallery_category = obj
                            .get("category")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    extract_tags(obj, &mut tags);
                }
            }
            _ => {
                // For backward compat with old line-delimited format where type code
                // wasn't present, try to parse as a file entry
                let metadata = &arr[arr.len() - 1];
                if let Some(obj) = metadata.as_object() {
                    if obj.contains_key("url") || obj.contains_key("filename") {
                        let file_url = obj
                            .get("url")
                            .or_else(|| obj.get("file_url"))
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        let filename = obj.get("filename").and_then(|v| v.as_str()).map(|f| {
                            if let Some(ext) = obj.get("extension").and_then(|e| e.as_str()) {
                                format!("{}.{}", f, ext)
                            } else {
                                f.to_string()
                            }
                        });

                        let filesize = obj
                            .get("filesize")
                            .or_else(|| obj.get("size"))
                            .and_then(|v| v.as_u64());

                        let width = obj.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
                        let height = obj.get("height").and_then(|v| v.as_u64()).map(|v| v as u32);

                        let file_thumb = obj
                            .get("thumbnail")
                            .or_else(|| obj.get("preview_url"))
                            .or_else(|| obj.get("sample_url"))
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        if thumbnail.is_none() {
                            thumbnail = file_thumb.clone().or_else(|| file_url.clone());
                        }

                        files.push(FileEntry {
                            url: file_url,
                            path: None,
                            filename,
                            filesize,
                            mime_type: obj.get("extension").and_then(|v| v.as_str()).and_then(
                                |ext| {
                                    crate::orchestrator::backends::guess_mime_type(&format!(
                                        "x.{}",
                                        ext
                                    ))
                                },
                            ),
                            thumbnail: file_thumb,
                            index: Some(file_idx),
                            width,
                            height,
                        });
                        file_idx += 1;

                        if gallery_title.is_none() {
                            gallery_title =
                                obj.get("title").and_then(|v| v.as_str()).map(String::from);
                        }
                        if gallery_artist.is_none() {
                            gallery_artist = obj
                                .get("artist")
                                .or_else(|| obj.get("author"))
                                .or_else(|| obj.get("uploader"))
                                .and_then(|v| match v {
                                    serde_json::Value::String(s) => Some(s.clone()),
                                    serde_json::Value::Object(o) => {
                                        o.get("name").and_then(|n| n.as_str()).map(String::from)
                                    }
                                    _ => None,
                                });
                        }
                        if gallery_category.is_none() {
                            gallery_category = obj
                                .get("category")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                        }
                        extract_tags(obj, &mut tags);
                    }
                }
            }
        }
    }

    if files.is_empty() {
        return Err(BackendError::ParseError(
            "gallery-dl returned no files".to_string(),
        ));
    }

    let file_count = files.len() as u32;
    let total_size: Option<u64> = {
        let sum: u64 = files.iter().filter_map(|f| f.filesize).sum();
        if sum > 0 {
            Some(sum)
        } else {
            None
        }
    };

    // Determine content type from actual file types, not just count.
    // gallery-dl can download anything: images, videos, archives, documents.
    let content_type = infer_content_type(&files);

    let title =
        gallery_title.or_else(|| gallery_category.as_ref().map(|c| format!("{} gallery", c)));

    let extractor = gallery_category
        .as_deref()
        .unwrap_or("gallery-dl")
        .to_string();

    Ok(UrlInfo {
        url: url.to_string(),
        extractor,
        content_type,
        title,
        thumbnail,
        filesize: total_size,
        tags: if tags.is_empty() { None } else { Some(tags) },
        files: Some(files),
        file_count: Some(file_count),
        gallery: Some(GalleryInfo {
            gallery_id: None,
            page_count: Some(file_count),
            artist: gallery_artist.clone(),
            circle: None,
            parody: None,
            characters: None,
            language: None,
            translated: None,
            convention: None,
        }),
        uploader: gallery_artist,
        ..Default::default()
    })
}

/// Parse gallery-dl stderr/stdout for progress information.
///
/// gallery-dl doesn't have a structured progress API like yt-dlp.
/// We track downloaded file count as a proxy for progress.
/// Since total isn't known, we use (downloaded, downloaded+1) so the
/// progress bar shows activity rather than being stuck at 0%.
fn parse_gallery_dl_progress(
    line: &str,
    job_id: &str,
    downloaded_count: &mut u64,
) -> Option<ProgressUpdate> {
    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }

    *downloaded_count += 1;

    Some(ProgressUpdate {
        job_id: job_id.to_string(),
        downloaded_bytes: *downloaded_count,
        total_bytes: Some(*downloaded_count + 1),
        speed: None,
        eta: None,
    })
}

/// Infer the best ContentType from the actual files gallery-dl found.
/// gallery-dl can download anything: images, videos, zips, documents, etc.
fn infer_content_type(files: &[FileEntry]) -> ContentType {
    if files.is_empty() {
        return ContentType::Other;
    }

    if files.len() == 1 {
        if let Some(ref mime) = files[0].mime_type {
            return mime_to_content_type(mime);
        }
        return ContentType::File;
    }

    let mut has_image = false;
    let mut has_video = false;
    let mut has_other = false;

    for file in files {
        match file.mime_type.as_deref() {
            Some(m) if m.starts_with("image/") => has_image = true,
            Some(m) if m.starts_with("video/") => has_video = true,
            Some(m) if m.starts_with("audio/") => has_other = true,
            Some(_) => has_other = true,
            None => has_other = true,
        }
    }

    if has_image && !has_video && !has_other {
        return ContentType::Gallery;
    }

    // Mixed content (kemono, patreon, etc.) — still a gallery/collection
    ContentType::Gallery
}

fn mime_to_content_type(mime: &str) -> ContentType {
    if mime.starts_with("image/") {
        ContentType::Image
    } else if mime.starts_with("video/") {
        ContentType::Video
    } else if mime.starts_with("audio/") {
        ContentType::Audio
    } else {
        ContentType::File
    }
}

fn parse_gallery_dl_error(stderr: &str) -> BackendError {
    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("no suitable extractor")
        || stderr_lower.contains("unsupported url")
        || stderr_lower.contains("no results")
    {
        return BackendError::UnsupportedUrl(stderr.trim().to_string());
    }

    if stderr_lower.contains("404") || stderr_lower.contains("not found") {
        return BackendError::NotFound(stderr.trim().to_string());
    }

    if stderr_lower.contains("403")
        || stderr_lower.contains("forbidden")
        || stderr_lower.contains("access denied")
    {
        return BackendError::Forbidden(stderr.trim().to_string());
    }

    if stderr_lower.contains("401")
        || stderr_lower.contains("unauthorized")
        || stderr_lower.contains("login")
    {
        return BackendError::Unauthorized(stderr.trim().to_string());
    }

    if stderr_lower.contains("429")
        || stderr_lower.contains("rate limit")
        || stderr_lower.contains("too many requests")
    {
        return BackendError::RateLimited(stderr.trim().to_string());
    }

    if stderr_lower.contains("network")
        || stderr_lower.contains("connection")
        || stderr_lower.contains("timeout")
        || stderr_lower.contains("dns")
    {
        return BackendError::NetworkError(stderr.trim().to_string());
    }

    BackendError::ProcessError(stderr.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_priority_gallery_domains() {
        assert!(GALLERY_DOMAINS
            .iter()
            .any(|d| "https://danbooru.donmai.us/posts/12345".contains(d)));
        assert!(GALLERY_DOMAINS
            .iter()
            .any(|d| "https://www.pixiv.net/artworks/12345".contains(d)));
        assert!(GALLERY_DOMAINS
            .iter()
            .any(|d| "https://kemono.cr/user/12345".contains(d)));
        assert!(!VIDEO_PRIMARY_DOMAINS
            .iter()
            .any(|d| "https://danbooru.donmai.us/posts/12345".contains(d)));
    }

    #[test]
    fn test_shared_domains() {
        assert!(SHARED_DOMAINS
            .iter()
            .any(|d| "https://twitter.com/user/status/123".contains(d)));
        assert!(SHARED_DOMAINS
            .iter()
            .any(|d| "https://www.reddit.com/r/art/post".contains(d)));
        assert!(SHARED_DOMAINS
            .iter()
            .any(|d| "https://www.instagram.com/p/abc".contains(d)));
    }

    #[test]
    fn test_video_primary_excluded() {
        assert!(VIDEO_PRIMARY_DOMAINS
            .iter()
            .any(|d| "https://www.youtube.com/watch?v=abc".contains(d)));
        assert!(VIDEO_PRIMARY_DOMAINS
            .iter()
            .any(|d| "https://www.twitch.tv/channel".contains(d)));
    }

    #[test]
    fn test_infer_content_type_images() {
        let files = vec![
            FileEntry {
                mime_type: Some("image/jpeg".into()),
                ..Default::default()
            },
            FileEntry {
                mime_type: Some("image/png".into()),
                ..Default::default()
            },
        ];
        assert_eq!(infer_content_type(&files), ContentType::Gallery);
    }

    #[test]
    fn test_infer_content_type_single_video() {
        let files = vec![FileEntry {
            mime_type: Some("video/mp4".into()),
            ..Default::default()
        }];
        assert_eq!(infer_content_type(&files), ContentType::Video);
    }

    #[test]
    fn test_infer_content_type_mixed() {
        let files = vec![
            FileEntry {
                mime_type: Some("image/jpeg".into()),
                ..Default::default()
            },
            FileEntry {
                mime_type: Some("application/zip".into()),
                ..Default::default()
            },
            FileEntry {
                mime_type: Some("video/mp4".into()),
                ..Default::default()
            },
        ];
        assert_eq!(infer_content_type(&files), ContentType::Gallery);
    }

    #[test]
    fn test_infer_content_type_single_file() {
        let files = vec![FileEntry {
            mime_type: Some("application/zip".into()),
            ..Default::default()
        }];
        assert_eq!(infer_content_type(&files), ContentType::File);
    }

    #[test]
    fn test_infer_content_type_empty() {
        assert_eq!(infer_content_type(&[]), ContentType::Other);
    }

    #[test]
    fn test_infer_content_type_single_image() {
        let files = vec![FileEntry {
            mime_type: Some("image/png".into()),
            ..Default::default()
        }];
        assert_eq!(infer_content_type(&files), ContentType::Image);
    }

    #[test]
    fn test_infer_content_type_single_audio() {
        let files = vec![FileEntry {
            mime_type: Some("audio/mp3".into()),
            ..Default::default()
        }];
        assert_eq!(infer_content_type(&files), ContentType::Audio);
    }

    #[test]
    fn test_infer_content_type_no_mime() {
        let files = vec![FileEntry {
            mime_type: None,
            ..Default::default()
        }];
        assert_eq!(infer_content_type(&files), ContentType::File);
    }

    #[test]
    fn test_mime_to_content_type_image() {
        assert_eq!(mime_to_content_type("image/jpeg"), ContentType::Image);
        assert_eq!(mime_to_content_type("image/png"), ContentType::Image);
        assert_eq!(mime_to_content_type("image/webp"), ContentType::Image);
    }

    #[test]
    fn test_mime_to_content_type_video() {
        assert_eq!(mime_to_content_type("video/mp4"), ContentType::Video);
        assert_eq!(mime_to_content_type("video/webm"), ContentType::Video);
    }

    #[test]
    fn test_mime_to_content_type_audio() {
        assert_eq!(mime_to_content_type("audio/mp3"), ContentType::Audio);
        assert_eq!(mime_to_content_type("audio/flac"), ContentType::Audio);
    }

    #[test]
    fn test_mime_to_content_type_other() {
        assert_eq!(mime_to_content_type("application/zip"), ContentType::File);
        assert_eq!(mime_to_content_type("application/pdf"), ContentType::File);
        assert_eq!(mime_to_content_type("text/plain"), ContentType::File);
    }

    #[test]
    fn test_extract_tags_array() {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"tags": ["tag1", "tag2", "tag3"]}"#).unwrap();
        let mut tags = Vec::new();
        extract_tags(&obj, &mut tags);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn test_extract_tags_string() {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"tags": "tag1 tag2 tag3"}"#).unwrap();
        let mut tags = Vec::new();
        extract_tags(&obj, &mut tags);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn test_extract_tags_no_duplicates() {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"tags": ["tag1", "tag2"]}"#).unwrap();
        let mut tags = vec!["tag1".to_string()];
        extract_tags(&obj, &mut tags);
        assert_eq!(tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn test_extract_tags_no_tags_field() {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"other": "value"}"#).unwrap();
        let mut tags = Vec::new();
        extract_tags(&obj, &mut tags);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_extract_tags_non_string_array_items() {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"tags": ["tag1", 42, "tag2"]}"#).unwrap();
        let mut tags = Vec::new();
        extract_tags(&obj, &mut tags);
        assert_eq!(tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn test_parse_gallery_dl_error_unsupported() {
        assert!(matches!(
            parse_gallery_dl_error("No suitable extractor found for 'https://example.com'"),
            BackendError::UnsupportedUrl(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("unsupported url scheme"),
            BackendError::UnsupportedUrl(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("No results found"),
            BackendError::UnsupportedUrl(_)
        ));
    }

    #[test]
    fn test_parse_gallery_dl_error_http_codes() {
        assert!(matches!(
            parse_gallery_dl_error("HttpError: 404 Not Found"),
            BackendError::NotFound(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("HttpError: 403 Forbidden"),
            BackendError::Forbidden(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("HttpError: 429 Too Many Requests"),
            BackendError::RateLimited(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("HttpError: 401 Unauthorized"),
            BackendError::Unauthorized(_)
        ));
    }

    #[test]
    fn test_parse_gallery_dl_error_login_required() {
        assert!(matches!(
            parse_gallery_dl_error("Login required to access this content"),
            BackendError::Unauthorized(_)
        ));
    }

    #[test]
    fn test_parse_gallery_dl_error_access_denied() {
        assert!(matches!(
            parse_gallery_dl_error("access denied: you need permission"),
            BackendError::Forbidden(_)
        ));
    }

    #[test]
    fn test_parse_gallery_dl_error_network() {
        assert!(matches!(
            parse_gallery_dl_error("Connection timeout after 30s"),
            BackendError::NetworkError(_)
        ));
        assert!(matches!(
            parse_gallery_dl_error("DNS resolution failed"),
            BackendError::NetworkError(_)
        ));
    }

    #[test]
    fn test_parse_gallery_dl_error_generic() {
        assert!(matches!(
            parse_gallery_dl_error("some unknown error"),
            BackendError::ProcessError(_)
        ));
    }

    #[test]
    fn test_parse_dump_json() {
        let json = r#"[[1, 2, "pixiv", "12345"], {"url": "https://example.com/img.jpg", "filename": "test", "extension": "jpg", "width": 1920, "height": 1080, "tags": ["tag1", "tag2"], "artist": "testartist"}]
[[1, 2, "pixiv", "12345"], {"url": "https://example.com/img2.png", "filename": "test2", "extension": "png", "width": 800, "height": 600}]"#;

        let result = parse_gallery_dl_dump_json(json, "https://pixiv.net/artworks/12345");
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.content_type, ContentType::Gallery);
        assert_eq!(info.file_count, Some(2));
        assert_eq!(info.files.as_ref().unwrap().len(), 2);
        assert_eq!(info.uploader.as_deref(), Some("testartist"));
        assert_eq!(info.tags.as_ref().unwrap().len(), 2);

        let gallery = info.gallery.unwrap();
        assert_eq!(gallery.page_count, Some(2));
        assert_eq!(gallery.artist.as_deref(), Some("testartist"));
    }

    #[test]
    fn test_parse_dump_json_single_array_format() {
        let json = r#"[
            [2, {"title": "Test Post", "category": "kemono", "username": "artist1", "tags": ["tag1"], "file": {"url": "https://example.com/thumb.jpg"}}],
            [3, "https://example.com/file1.zip", {"url": "https://example.com/file1.zip", "filename": "archive", "extension": "zip", "category": "kemono", "title": "Test Post"}],
            [3, "https://example.com/movie.mp4", {"url": "https://example.com/movie.mp4", "filename": "Movie_1", "extension": "mp4", "category": "kemono"}],
            [3, "https://example.com/cover.jpg", {"url": "https://example.com/cover.jpg", "filename": "cover", "extension": "jpg", "category": "kemono"}]
        ]"#;

        let result = parse_gallery_dl_dump_json(json, "https://kemono.cr/fanbox/user/123/post/456");
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let info = result.unwrap();
        assert_eq!(info.file_count, Some(3));
        assert_eq!(info.files.as_ref().unwrap().len(), 3);
        assert_eq!(info.title.as_deref(), Some("Test Post"));
        assert_eq!(info.uploader.as_deref(), Some("artist1"));
        assert_eq!(
            info.thumbnail.as_deref(),
            Some("https://example.com/thumb.jpg")
        );
        assert_eq!(info.content_type, ContentType::Gallery);

        let files = info.files.unwrap();
        assert_eq!(files[0].filename.as_deref(), Some("archive.zip"));
        assert_eq!(files[1].filename.as_deref(), Some("Movie_1.mp4"));
        assert_eq!(files[2].filename.as_deref(), Some("cover.jpg"));
        assert_eq!(files[0].index, Some(0));
        assert_eq!(files[1].index, Some(1));
        assert_eq!(files[2].index, Some(2));
    }

    #[test]
    fn test_parse_dump_json_single_image() {
        let json = r#"[
            [2, {"title": "Single Image", "category": "danbooru"}],
            [3, "https://example.com/img.jpg", {"url": "https://example.com/img.jpg", "filename": "image", "extension": "jpg"}]
        ]"#;
        let result = parse_gallery_dl_dump_json(json, "https://example.com/img.jpg");
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.content_type, ContentType::Image);
        assert_eq!(info.file_count, Some(1));
    }

    #[test]
    fn test_parse_dump_json_single_image_line_delimited() {
        let json = r#"[[1, 2], {"url": "https://example.com/img.jpg", "filename": "image", "extension": "jpg"}]"#;
        let result = parse_gallery_dl_dump_json(json, "https://example.com/img.jpg");
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.content_type, ContentType::Image);
        assert_eq!(info.file_count, Some(1));
    }

    #[test]
    fn test_parse_dump_json_empty() {
        let result = parse_gallery_dl_dump_json("", "https://example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_progress() {
        let mut count = 0u64;
        let result = parse_gallery_dl_progress("/path/to/file.jpg", "job1", &mut count);
        assert!(result.is_some());
        assert_eq!(count, 1);
        let p = result.unwrap();
        assert_eq!(p.downloaded_bytes, 1);
        assert_eq!(p.total_bytes, Some(2));

        let result = parse_gallery_dl_progress("/path/to/file2.jpg", "job1", &mut count);
        assert!(result.is_some());
        assert_eq!(count, 2);
        let p = result.unwrap();
        assert_eq!(p.downloaded_bytes, 2);
        assert_eq!(p.total_bytes, Some(3));

        let result = parse_gallery_dl_progress("# https://example.com", "job1", &mut count);
        assert!(result.is_none());
        assert_eq!(count, 2);
    }

    #[test]
    fn test_parse_progress_skip_empty() {
        let mut count = 0u64;
        assert!(parse_gallery_dl_progress("", "job1", &mut count).is_none());
        assert!(parse_gallery_dl_progress("   ", "job1", &mut count).is_none());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_parse_progress_skip_bracketed() {
        let mut count = 0u64;
        assert!(parse_gallery_dl_progress("[info] Processing...", "job1", &mut count).is_none());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_parse_progress_job_id() {
        let mut count = 0u64;
        let result = parse_gallery_dl_progress("/tmp/img.jpg", "my-gallery-job", &mut count);
        assert_eq!(result.unwrap().job_id, "my-gallery-job");
    }

    #[test]
    fn test_parse_progress_multiple_files() {
        let mut count = 0u64;
        for i in 1..=10 {
            let line = format!("/path/to/file_{}.jpg", i);
            let result = parse_gallery_dl_progress(&line, "job1", &mut count);
            assert!(result.is_some());
            assert_eq!(count, i);
            let p = result.unwrap();
            assert_eq!(p.downloaded_bytes, i);
            assert_eq!(p.total_bytes, Some(i + 1));
        }
    }
}
