//! Thumbnail helpers for the orchestrator pipeline (duplicates `lib.rs` for now).

use tauri::{AppHandle, Manager};

use crate::orchestrator::types::BackendError;

#[cfg(not(target_os = "android"))]
use image::DynamicImage;

#[cfg(not(target_os = "android"))]
use image::GenericImageView;

#[cfg(target_os = "windows")]
use crate::utils::CommandHideConsole;

/// Embed cover art (best-effort).
#[cfg(not(target_os = "android"))]
pub async fn embed_thumbnail(
    app: &AppHandle,
    audio_path: &str,
    thumbnail_url: &str,
) -> Result<String, BackendError> {
    embed_cover_art(app, audio_path, thumbnail_url).await
}

/// Embed cover art (best-effort).
#[cfg(not(target_os = "android"))]
pub async fn embed_video_thumbnail(
    app: &AppHandle,
    video_path: &str,
    thumbnail_url: &str,
) -> Result<String, BackendError> {
    embed_cover_art(app, video_path, thumbnail_url).await
}

#[cfg(target_os = "android")]
pub async fn embed_video_thumbnail(
    _app: &AppHandle,
    video_path: &str,
    _thumbnail_url: &str,
) -> Result<String, BackendError> {
    Ok(video_path.to_string())
}

#[cfg(target_os = "android")]
pub async fn embed_thumbnail(
    _app: &AppHandle,
    audio_path: &str,
    _thumbnail_url: &str,
) -> Result<String, BackendError> {
    Ok(audio_path.to_string())
}

/// Single entrypoint for cover art embedding.
/// May remux: opus→ogg, webm→mkv.
#[cfg(not(target_os = "android"))]
pub async fn embed_cover_art(
    app: &AppHandle,
    media_path: &str,
    thumbnail_url: &str,
) -> Result<String, BackendError> {
    let ext = std::path::Path::new(media_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Rough classification by common container extensions.
    let is_audio = matches!(
        ext.as_str(),
        "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus" | "mka"
    );
    let is_video = matches!(
        ext.as_str(),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "ts" | "mts" | "flv" | "wmv" | "3gp"
    );

    let result = if is_video {
        embed_video_thumbnail_from_url(app, media_path, thumbnail_url).await
    } else if is_audio {
        embed_thumbnail_from_url(app, media_path, thumbnail_url).await
    } else {
        // Unknown container: prefer audio path first (more conservative), then video.
        match embed_thumbnail_from_url(app, media_path, thumbnail_url).await {
            Ok(p) => Ok(p),
            Err(e1) => match embed_video_thumbnail_from_url(app, media_path, thumbnail_url).await {
                Ok(p) => Ok(p),
                Err(_e2) => Err(e1),
            },
        }
    };

    result.map_err(BackendError::Other)
}

#[cfg(target_os = "android")]
pub async fn embed_cover_art(
    _app: &AppHandle,
    media_path: &str,
    _thumbnail_url: &str,
) -> Result<String, BackendError> {
    Ok(media_path.to_string())
}

#[cfg(not(target_os = "android"))]
fn is_letterboxed_thumbnail(img: &DynamicImage) -> bool {
    let (width, height) = img.dimensions();

    if width <= height {
        return false;
    }

    let square_size = height;
    let bar_width = (width - square_size) / 2;

    if bar_width < (width / 20) {
        return false;
    }

    let dark_threshold: u8 = 30;

    let sample_points_left = [
        (bar_width / 4, height / 4),
        (bar_width / 4, height / 2),
        (bar_width / 4, height * 3 / 4),
        (bar_width / 2, height / 4),
        (bar_width / 2, height / 2),
        (bar_width / 2, height * 3 / 4),
        (bar_width * 3 / 4, height / 4),
        (bar_width * 3 / 4, height / 2),
        (bar_width * 3 / 4, height * 3 / 4),
    ];

    let sample_points_right = [
        (width - bar_width / 4, height / 4),
        (width - bar_width / 4, height / 2),
        (width - bar_width / 4, height * 3 / 4),
        (width - bar_width / 2, height / 4),
        (width - bar_width / 2, height / 2),
        (width - bar_width / 2, height * 3 / 4),
        (width - bar_width * 3 / 4, height / 4),
        (width - bar_width * 3 / 4, height / 2),
        (width - bar_width * 3 / 4, height * 3 / 4),
    ];

    let mut dark_count = 0;
    let total_samples = sample_points_left.len() + sample_points_right.len();

    for (x, y) in sample_points_left.iter().chain(sample_points_right.iter()) {
        if *x >= width || *y >= height {
            continue;
        }
        let pixel = img.get_pixel(*x, *y);
        if pixel[0] <= dark_threshold && pixel[1] <= dark_threshold && pixel[2] <= dark_threshold {
            dark_count += 1;
        }
    }

    let required_dark = (total_samples * 7) / 10;
    if dark_count >= required_dark {
        return true;
    }

    let tolerance: i16 = 60;
    let ref_color = img.get_pixel(bar_width / 2, height / 2);

    let mut uniform_count = 0;
    for (x, y) in sample_points_left.iter().chain(sample_points_right.iter()) {
        if *x >= width || *y >= height {
            continue;
        }
        let pixel = img.get_pixel(*x, *y);

        let diff_r = (pixel[0] as i16 - ref_color[0] as i16).abs();
        let diff_g = (pixel[1] as i16 - ref_color[1] as i16).abs();
        let diff_b = (pixel[2] as i16 - ref_color[2] as i16).abs();

        if diff_r <= tolerance && diff_g <= tolerance && diff_b <= tolerance {
            uniform_count += 1;
        }
    }

    let required_uniform = (total_samples * 7) / 10;

    uniform_count >= required_uniform
}

#[cfg(not(target_os = "android"))]
fn crop_to_center_square(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let square_size = height;
    let x_offset = (width - square_size) / 2;

    img.crop_imm(x_offset, 0, square_size, square_size)
}

#[cfg(not(target_os = "android"))]
async fn embed_thumbnail_from_url(
    app: &AppHandle,
    audio_path: &str,
    thumbnail_url: &str,
) -> Result<String, String> {
    use std::io::Cursor;

    if thumbnail_url.is_empty() {
        return Err("Empty thumbnail URL".to_string());
    }

    let response = reqwest::get(thumbnail_url)
        .await
        .map_err(|e| format!("Failed to download thumbnail: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read thumbnail bytes: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to decode image: {}", e))?;

    let processed = if is_letterboxed_thumbnail(&img) {
        crop_to_center_square(img)
    } else {
        img
    };

    let mut jpeg_bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut jpeg_bytes);
    processed
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode thumbnail as JPEG: {}", e))?;

    embed_thumbnail_jpeg_bytes(app, audio_path, &jpeg_bytes).await
}

#[cfg(not(target_os = "android"))]
async fn embed_video_thumbnail_from_url(
    app: &AppHandle,
    video_path: &str,
    thumbnail_url: &str,
) -> Result<String, String> {
    use std::io::Cursor;

    if thumbnail_url.is_empty() {
        return Err("Empty thumbnail URL".to_string());
    }

    let response = reqwest::get(thumbnail_url)
        .await
        .map_err(|e| format!("Failed to download thumbnail: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read thumbnail bytes: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to decode image: {}", e))?;

    let processed = if is_letterboxed_thumbnail(&img) {
        crop_to_center_square(img)
    } else {
        img
    };

    let mut jpeg_bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut jpeg_bytes);
    processed
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode thumbnail as JPEG: {}", e))?;

    embed_video_thumbnail_jpeg_bytes(app, video_path, &jpeg_bytes).await
}

#[cfg(not(target_os = "android"))]
async fn embed_video_thumbnail_jpeg_bytes(
    app: &AppHandle,
    video_path: &str,
    jpeg_bytes: &[u8],
) -> Result<String, String> {
    use std::process::Stdio;
    use std::time::{SystemTime, UNIX_EPOCH};

    let video_path_buf = std::path::PathBuf::from(video_path);
    let video_ext = video_path_buf
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

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

    let ffmpeg_path = crate::deps::get_ffmpeg_path(app)?;
    if !ffmpeg_path.exists() {
        let _ = tokio::fs::remove_file(&thumb_path).await;
        return Err("FFmpeg not found".to_string());
    }

    // webm doesn't support attached pictures; remux to mkv for cover art.
    let output_ext = if video_ext == "webm" { "mkv" } else { video_ext.as_str() };

    let temp_output = video_path_buf.with_extension(format!("temp.{}", output_ext));
    let final_output = if video_ext == "webm" {
        video_path_buf.with_extension("mkv")
    } else {
        video_path_buf.clone()
    };

    let mut cmd = tokio::process::Command::new(&ffmpeg_path);
    cmd.args([
        "-y",
        "-i",
        video_path,
        "-i",
        thumb_path
            .to_str()
            .ok_or("Invalid thumbnail path encoding")?,
        "-map",
        "0",
        "-map",
        "1",
        "-c",
        "copy",
        "-c:v:1",
        "mjpeg",
        "-disposition:v:1",
        "attached_pic",
        "-metadata:s:v:1",
        "title=Album cover",
        "-metadata:s:v:1",
        "comment=Cover (front)",
    ]);

    cmd.arg(temp_output.to_str().ok_or("Invalid output path encoding")?);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.hide_console();

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    let _ = tokio::fs::remove_file(&thumb_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_file(&temp_output).await;
        return Err(format!("FFmpeg failed to embed video thumbnail: {}", stderr));
    }

    tokio::fs::rename(&temp_output, &final_output)
        .await
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    if video_ext == "webm" {
        let _ = tokio::fs::remove_file(video_path).await;
    }

    Ok(final_output.to_string_lossy().to_string())
}

#[cfg(not(target_os = "android"))]
async fn embed_thumbnail_jpeg_bytes(
    app: &AppHandle,
    audio_path: &str,
    jpeg_bytes: &[u8],
) -> Result<String, String> {
    use std::process::Stdio;
    use std::time::{SystemTime, UNIX_EPOCH};

    let audio_path_buf = std::path::PathBuf::from(audio_path);
    let audio_ext = audio_path_buf
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

    let ffmpeg_path = crate::deps::get_ffmpeg_path(app)?;
    if !ffmpeg_path.exists() {
        let _ = tokio::fs::remove_file(&thumb_path).await;
        return Err("FFmpeg not found".to_string());
    }

    // For opus files, output to ogg container (supports cover art) instead of opus muxer (doesn't)
    let output_ext = if audio_ext == "opus" {
        "ogg".to_string()
    } else {
        audio_ext.clone()
    };
    let temp_output = audio_path_buf.with_extension(format!("temp.{}", output_ext));
    let final_output = if audio_ext == "opus" {
        audio_path_buf.with_extension("ogg")
    } else {
        audio_path_buf.clone()
    };

    let mut cmd = tokio::process::Command::new(&ffmpeg_path);
    cmd.args([
        "-y",
        "-i",
        audio_path,
        "-i",
        thumb_path
            .to_str()
            .ok_or("Invalid thumbnail path encoding")?,
        "-map",
        "0:a",
        "-map",
        "1:v",
        "-c:a",
        "copy",
        "-c:v",
        "mjpeg",
    ]);

    if audio_ext == "mp3" {
        cmd.args([
            "-id3v2_version",
            "3",
            "-metadata:s:v",
            "title=Album cover",
            "-metadata:s:v",
            "comment=Cover (front)",
        ]);
    } else {
        cmd.args(["-disposition:v:0", "attached_pic"]);
    }

    cmd.arg(temp_output.to_str().ok_or("Invalid output path encoding")?);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.hide_console();

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

    tokio::fs::rename(&temp_output, &final_output)
        .await
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    if audio_ext == "opus" {
        let _ = tokio::fs::remove_file(audio_path).await;
    }

    Ok(final_output.to_string_lossy().to_string())
}

#[cfg(not(target_os = "android"))]
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
        use std::process::Stdio;

        let ffmpeg_path = crate::deps::get_ffmpeg_path(app)?;

        let ffmpeg_cmd = if ffmpeg_path.exists() {
            ffmpeg_path.to_string_lossy().to_string()
        } else {
            "ffmpeg".to_string()
        };

        let mut cmd = tokio::process::Command::new(&ffmpeg_cmd);
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

        #[cfg(target_os = "windows")]
        cmd.hide_console();

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
    } else {
        Err(format!("Unsupported file type: {}", extension))
    }
}

#[cfg(target_os = "android")]
pub async fn generate_local_thumbnail(
    _app: &AppHandle,
    _file_path: &str,
    _item_id: &str,
) -> Result<String, String> {
    Err("Local thumbnail generation is not supported on Android".to_string())
}
