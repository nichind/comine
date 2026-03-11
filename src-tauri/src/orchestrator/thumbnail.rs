use tauri::{AppHandle, Manager};

use crate::orchestrator::types::BackendError;

use image::DynamicImage;
use image::GenericImageView;

fn supports_embedded_thumbnail(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "mp4" | "m4a" | "m4v" | "mov" | "mkv" | "flac" | "ogg" | "avi"
    )
}

pub async fn embed_thumbnail(
    app: &AppHandle,
    media_path: &str,
    thumbnail_url: &str,
) -> Result<String, BackendError> {
    let (path, _jpeg_bytes) = embed_thumbnail_from_url(app, media_path, thumbnail_url)
        .await
        .map_err(BackendError::Other)?;
    Ok(path)
}

mod letterbox {
    pub const MIN_ASPECT_RATIO: f32 = 1.6;
    pub const MAX_ASPECT_RATIO: f32 = 1.9;
    pub const UNIFORMITY_THRESHOLD_PERCENT: usize = 85;
    pub const COLOR_TOLERANCE: i16 = 25;
}

fn is_letterboxed_thumbnail(img: &DynamicImage) -> bool {
    use tracing::debug;

    let (width, height) = img.dimensions();

    if width <= height {
        debug!(target: "thumbnail", "Not letterboxed: width {} <= height {}", width, height);
        return false;
    }

    let square_size = height;
    let bar_width = (width - square_size) / 2;

    if bar_width < (width / 20) {
        debug!(target: "thumbnail", "Not letterboxed: bar_width {} too small (< {})", bar_width, width / 20);
        return false;
    }

    // Require aspect ratio close to 16:9 (1.777...), allow configurable range
    let aspect = width as f32 / height as f32;
    if !(letterbox::MIN_ASPECT_RATIO..=letterbox::MAX_ASPECT_RATIO).contains(&aspect) {
        debug!(target: "thumbnail", "Not letterboxed: aspect ratio {} not in 16:9 range", aspect);
        return false;
    }

    let sample_points: Vec<(u32, u32)> = [
        (bar_width / 4, height / 4),
        (bar_width / 4, height / 2),
        (bar_width / 4, height * 3 / 4),
        (bar_width / 2, height / 4),
        (bar_width / 2, height / 2),
        (bar_width / 2, height * 3 / 4),
        (bar_width * 3 / 4, height / 4),
        (bar_width * 3 / 4, height / 2),
        (bar_width * 3 / 4, height * 3 / 4),
        (width - bar_width / 4, height / 4),
        (width - bar_width / 4, height / 2),
        (width - bar_width / 4, height * 3 / 4),
        (width - bar_width / 2, height / 4),
        (width - bar_width / 2, height / 2),
        (width - bar_width / 2, height * 3 / 4),
        (width - bar_width * 3 / 4, height / 4),
        (width - bar_width * 3 / 4, height / 2),
        (width - bar_width * 3 / 4, height * 3 / 4),
    ]
    .into_iter()
    .filter(|(x, y)| *x < width && *y < height)
    .collect();

    if sample_points.is_empty() {
        return false;
    }

    let ref_color = img.get_pixel(bar_width / 2, height / 2);
    let tolerance: i16 = letterbox::COLOR_TOLERANCE;

    let uniform_count = sample_points
        .iter()
        .filter(|(x, y)| {
            let pixel = img.get_pixel(*x, *y);
            let diff_r = (pixel[0] as i16 - ref_color[0] as i16).abs();
            let diff_g = (pixel[1] as i16 - ref_color[1] as i16).abs();
            let diff_b = (pixel[2] as i16 - ref_color[2] as i16).abs();
            diff_r <= tolerance && diff_g <= tolerance && diff_b <= tolerance
        })
        .count();

    let required = (sample_points.len() * letterbox::UNIFORMITY_THRESHOLD_PERCENT) / 100;
    let is_letterboxed = uniform_count >= required;

    debug!(target: "thumbnail", "Letterbox: {} of {} uniform (need {}), result: {}",
           uniform_count, sample_points.len(), required, is_letterboxed);
    is_letterboxed
}

#[allow(dead_code)]
pub fn is_letterboxed_image_path(path: &str) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read image file: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode image from {path}: {e}"))?;
    Ok(is_letterboxed_thumbnail(&img))
}

fn crop_to_center_square(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let square_size = height;
    let x_offset = (width - square_size) / 2;

    img.crop_imm(x_offset, 0, square_size, square_size)
}

fn cache_filename(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(url.as_bytes());
    format!("{}.jpg", hex::encode(&hash[..12]))
}

async fn fetch_and_process_thumbnail(url: &str) -> Result<DynamicImage, String> {
    let is_local = std::path::Path::new(url).is_absolute();

    let raw_bytes = if is_local {
        tokio::fs::read(url)
            .await
            .map_err(|e| format!("Failed to read local image: {e}"))?
    } else {
        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to fetch thumbnail: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read thumbnail bytes: {e}"))?
            .to_vec()
    };

    let img =
        image::load_from_memory(&raw_bytes).map_err(|e| format!("Failed to decode image: {e}"))?;

    let processed = if is_letterboxed_thumbnail(&img) {
        crop_to_center_square(img)
    } else {
        img
    };

    Ok(processed)
}

fn encode_thumbnail_jpeg(img: &DynamicImage, max_size: u32) -> Result<Vec<u8>, String> {
    let thumb = img.thumbnail(max_size, max_size);
    let rgb = thumb.to_rgb8();
    let mut buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    rgb.write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode thumbnail as JPEG: {e}"))?;
    Ok(buf)
}

async fn save_to_thumbnail_cache(app: &AppHandle, url: &str, jpeg_bytes: &[u8]) {
    let cache_dir = match thumbnail_cache_dir(app) {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = tokio::fs::create_dir_all(&cache_dir).await;
    let path = cache_dir.join(cache_filename(url));
    if let Err(e) = tokio::fs::write(&path, jpeg_bytes).await {
        tracing::debug!(target: "thumbnail", "Failed to save thumbnail cache for {}: {}", url, e);
    } else {
        tracing::debug!(target: "thumbnail", "Saved cached thumbnail: {}", path.display());
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeIsLetterboxed<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    image_path: jni::objects::JString<'local>,
) -> jni::sys::jboolean {
    let path: String = match env.get_string(&image_path) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    match is_letterboxed_image_path(&path) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            tracing::warn!("nativeIsLetterboxed failed for {}: {}", path, e);
            0
        }
    }
}

async fn embed_thumbnail_from_url(
    app: &AppHandle,
    media_path: &str,
    thumbnail_url: &str,
) -> Result<(String, Option<Vec<u8>>), String> {
    if thumbnail_url.is_empty() {
        return Err("Empty thumbnail URL".to_string());
    }

    // Mobile platforms don't support thumbnail embedding into media files via ffmpeg.
    #[cfg(mobile)]
    {
        let _ = app;
        return Ok((media_path.to_string(), None));
    }

    #[cfg(desktop)]
    {
        // Skip formats that don't support embedded thumbnails
        let ext = std::path::Path::new(media_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        if !supports_embedded_thumbnail(ext) {
            tracing::info!(target: "thumbnail", "Skipping thumbnail embedding for .{} (format doesn't support it)", ext);
            return Ok((media_path.to_string(), None));
        }

        let processed = fetch_and_process_thumbnail(thumbnail_url).await?;

        let jpeg_bytes =
            encode_thumbnail_jpeg(&processed, processed.width().max(processed.height()))?;

        let result = embed_thumbnail_jpeg_bytes(app, media_path, &jpeg_bytes).await?;

        // Save a 320px version to the thumbnail cache so the UI is in sync
        match encode_thumbnail_jpeg(&processed, 320) {
            Ok(cache_bytes) => {
                save_to_thumbnail_cache(app, thumbnail_url, &cache_bytes).await;
            }
            Err(e) => {
                tracing::debug!(target: "thumbnail", "Failed to encode cache thumbnail: {}", e);
            }
        }

        Ok((result, Some(jpeg_bytes)))
    }
}

#[cfg(desktop)]
async fn embed_thumbnail_jpeg_bytes(
    app: &AppHandle,
    media_path: &str,
    jpeg_bytes: &[u8],
) -> Result<String, String> {
    use std::process::Stdio;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn count_video_streams(app: &AppHandle, file_path: &str) -> Result<usize, String> {
        use tokio::io::AsyncReadExt;

        let ffprobe_path = crate::deps::resolve_ffprobe_path(app);

        let ffprobe_cmd = match ffprobe_path {
            Some(p) => p.to_string_lossy().to_string(),
            None => "ffprobe".to_string(),
        };

        let mut cmd = crate::utils::new_command(&ffprobe_cmd);
        cmd.args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
            file_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        let mut stdout = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_end(&mut stdout).await;
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for ffprobe: {}", e))?;

        if !status.success() {
            return Ok(0);
        }

        let s = String::from_utf8_lossy(&stdout);
        let count = s
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .count();

        Ok(count)
    }

    let media_path_buf = std::path::PathBuf::from(media_path);
    let media_ext = media_path_buf
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "mp3".to_string());

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let thumb_path = cache_dir.join(format!("cover_{}.jpg", stamp));
    tokio::fs::write(&thumb_path, jpeg_bytes)
        .await
        .map_err(|e| format!("Failed to write thumbnail file: {}", e))?;

    let ffmpeg_path = match crate::deps::resolve_ffmpeg_path(app) {
        Some(path) => path,
        None => {
            let _ = tokio::fs::remove_file(&thumb_path).await;
            return Err("FFmpeg not found".to_string());
        }
    };

    let temp_output = media_path_buf.with_extension(format!("temp.{}", media_ext));
    let final_output = media_path_buf.clone();

    let existing_video_streams = count_video_streams(app, media_path).await.unwrap_or(0);
    // We map all input streams first, then add the thumbnail image as an extra video stream.
    // Therefore the cover art becomes v:{existing_video_streams} in the output.
    let cover_video_index: usize = existing_video_streams;

    let mut cmd = crate::utils::new_command(&ffmpeg_path);
    cmd.args([
        "-y",
        "-i",
        media_path,
        "-i",
        thumb_path
            .to_str()
            .ok_or("Invalid thumbnail path encoding")?,
        "-map",
        "0",
        "-map",
        "1:v",
        "-map_metadata",
        "0",
        "-map_chapters",
        "0",
        "-c",
        "copy",
    ]);

    cmd.arg(format!("-c:v:{}", cover_video_index));
    cmd.arg("mjpeg");
    cmd.arg(format!("-metadata:s:v:{}", cover_video_index));
    cmd.arg("title=Album cover");
    cmd.arg(format!("-metadata:s:v:{}", cover_video_index));
    cmd.arg("comment=Cover (front)");

    if media_ext == "mp3" {
        cmd.args(["-id3v2_version", "3", "-write_id3v1", "1"]);
    } else {
        cmd.arg(format!("-disposition:v:{}", cover_video_index));
        cmd.arg("attached_pic");
    }

    cmd.arg(temp_output.to_str().ok_or("Invalid output path encoding")?);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    let _ = tokio::fs::remove_file(&thumb_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_file(&temp_output).await;
        return Err(format!("FFmpeg failed to embed thumbnail: {}", stderr));
    }

    if final_output == media_path_buf {
        let file_name = media_path_buf
            .file_name()
            .ok_or("Invalid input filename")?
            .to_string_lossy();
        let bak_path = media_path_buf.with_file_name(format!("{}.bak", file_name));

        tokio::fs::rename(&media_path_buf, &bak_path)
            .await
            .map_err(|e| format!("Failed to create backup file: {}", e))?;

        if let Err(e) = tokio::fs::rename(&temp_output, &final_output).await {
            let _ = tokio::fs::rename(&bak_path, &media_path_buf).await;
            let _ = tokio::fs::remove_file(&temp_output).await;
            return Err(format!("Failed to replace original file: {}", e));
        }

        let _ = tokio::fs::remove_file(&bak_path).await;
    }

    Ok(final_output.to_string_lossy().to_string())
}

pub async fn generate_local_thumbnail(
    app: &AppHandle,
    file_path: &str,
    item_id: &str,
) -> Result<String, String> {
    use std::io::Cursor;

    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?
        .join("thumbnails");

    if !cache_dir.exists() {
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(|e| format!("Failed to create thumbnail cache dir: {}", e))?;
    }

    let output_path = cache_dir.join(format!("{}.jpg", item_id));

    if output_path.exists() {
        return Ok(format!("file://{}", output_path.to_string_lossy()));
    }

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let is_image = matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tiff" | "tif"
    );
    let is_video = matches!(
        extension.as_str(),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "3gp" | "ts" | "mts"
    );

    if is_image {
        let bytes = tokio::fs::read(&file_path)
            .await
            .map_err(|e| format!("Failed to read image: {}", e))?;

        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let thumbnail = img.thumbnail(320, 180);

        let rgb_img = thumbnail.to_rgb8();
        let mut jpeg_bytes: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut jpeg_bytes);

        rgb_img
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;

        tokio::fs::write(&output_path, jpeg_bytes)
            .await
            .map_err(|e| format!("Failed to write thumbnail: {}", e))?;

        Ok(format!("file://{}", output_path.to_string_lossy()))
    } else if is_video {
        #[cfg(desktop)]
        {
            use std::process::Stdio;

            let ffmpeg_path = crate::deps::resolve_ffmpeg_path(app);

            let ffmpeg_cmd = match ffmpeg_path {
                Some(p) => p.to_string_lossy().to_string(),
                None => "ffmpeg".to_string(),
            };

            let mut cmd = crate::utils::new_command(&ffmpeg_cmd);
            cmd.args([
                "-y",
                "-ss",
                "2",
                "-i",
                file_path,
                "-vframes",
                "1",
                "-vf",
                "scale=320:-2",
                "-q:v",
                "5",
                output_path.to_str().unwrap_or(""),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

            let output = cmd
                .output()
                .await
                .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("ffmpeg failed: {}", stderr));
            }
            if !output_path.exists() {
                return Err("Failed to generate video thumbnail".to_string());
            }

            Ok(format!("file://{}", output_path.to_string_lossy()))
        }
        #[cfg(mobile)]
        {
            Err(format!(
                "Video thumbnail generation not supported on mobile"
            ))
        }
    } else {
        Err(format!("Unsupported file type: {}", extension))
    }
}

const THUMBNAIL_CACHE_MAX_BYTES: u64 = 50 * 1024 * 1024;

const THUMBNAILS_DIR: &str = "thumbnails";

fn thumbnail_cache_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))
        .map(|d| d.join(THUMBNAILS_DIR))
}

#[tauri::command]
pub async fn get_or_cache_thumbnail(
    app: AppHandle,
    url: String,
    item_id: String,
) -> Result<String, String> {
    let cache_dir = thumbnail_cache_dir(&app)?;

    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| format!("Failed to create thumbnail cache dir: {}", e))?;

    let output_path = cache_dir.join(cache_filename(&url));

    if output_path.exists() {
        tracing::debug!(target: "thumbnail", "Cache hit for {} ({}): {}", item_id, url, output_path.display());
        return Ok(output_path.to_string_lossy().to_string());
    }

    tracing::debug!(target: "thumbnail", "Cache miss for {} ({}), downloading...", item_id, url);

    let processed = fetch_and_process_thumbnail(&url).await?;
    let jpeg_bytes = encode_thumbnail_jpeg(&processed, 320)?;

    tokio::fs::write(&output_path, &jpeg_bytes)
        .await
        .map_err(|e| format!("Failed to write cached thumbnail: {}", e))?;

    tracing::debug!(target: "thumbnail", "Cached thumbnail for {} ({}): {}", item_id, url, output_path.display());

    Ok(output_path.to_string_lossy().to_string())
}

pub async fn prune_thumbnail_cache(app: &AppHandle) {
    let cache_dir = match app.path().app_cache_dir() {
        Ok(d) => d.join(THUMBNAILS_DIR),
        Err(_) => return,
    };

    if !cache_dir.exists() {
        return;
    }

    let mut entries: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();

    let mut read_dir = match tokio::fs::read_dir(&cache_dir).await {
        Ok(rd) => rd,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jpg") {
            continue;
        }
        if let Ok(meta) = entry.metadata().await {
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            entries.push((path, meta.len(), modified));
        }
    }

    let total_size: u64 = entries.iter().map(|(_, sz, _)| sz).sum();

    if total_size <= THUMBNAIL_CACHE_MAX_BYTES {
        tracing::debug!(
            target: "thumbnail",
            "Thumbnail cache size: {} bytes ({} files), within {} limit",
            total_size,
            entries.len(),
            THUMBNAIL_CACHE_MAX_BYTES
        );
        return;
    }

    entries.sort_by_key(|(_, _, modified)| *modified);

    let mut freed: u64 = 0;
    let target_free = total_size - THUMBNAIL_CACHE_MAX_BYTES;

    for (path, size, _) in &entries {
        if freed >= target_free {
            break;
        }
        if tokio::fs::remove_file(path).await.is_ok() {
            freed += size;
            tracing::debug!(target: "thumbnail", "Pruned cached thumbnail: {}", path.display());
        }
    }

    tracing::info!(
        target: "thumbnail",
        "Thumbnail cache pruned: freed {} bytes, was {} bytes",
        freed,
        total_size
    );
}
