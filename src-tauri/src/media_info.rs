use tauri::AppHandle;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[cfg(not(target_os = "android"))]
fn resolve_ffprobe_cmd(app: &AppHandle) -> Result<String, String> {
    match crate::deps::resolve_ffprobe_path(app) {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Ok("ffprobe".to_string()),
    }
}

#[cfg(not(target_os = "android"))]
async fn run_ffprobe(app: &AppHandle, args: &[&str]) -> Result<String, String> {
    use std::process::Stdio;

    let cmd_str = resolve_ffprobe_cmd(app)?;
    let mut cmd = crate::utils::new_command(&cmd_str);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn get_media_duration(app: AppHandle, file_path: String) -> Result<f64, String> {
    #[cfg(target_os = "android")]
    {
        return Err("get_media_duration not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let ffprobe_cmd = resolve_ffprobe_cmd(&app)?;
        crate::utils::get_media_duration(std::path::Path::new(&ffprobe_cmd), &file_path)
            .await
            .ok_or_else(|| format!("Failed to get duration for: {}", file_path))
    }
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MediaFormatInfo {
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub bit_rate: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MediaStreamInfo {
    pub index: Option<u64>,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub profile: Option<String>,

    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,

    pub width: Option<u32>,
    pub height: Option<u32>,
    pub r_frame_rate: Option<String>,
    pub avg_frame_rate: Option<String>,

    pub bit_rate: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MediaTechnicalInfo {
    pub format: Option<MediaFormatInfo>,
    pub streams: Vec<MediaStreamInfo>,
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn get_media_technical_info(
    app: AppHandle,
    file_path: String,
) -> Result<MediaTechnicalInfo, String> {
    #[cfg(target_os = "android")]
    {
        return Err("get_media_technical_info not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let stdout = run_ffprobe(
            &app,
            &[
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                &file_path,
            ],
        )
        .await?;

        let value: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse ffprobe JSON: {}", e))?;

        use crate::orchestrator::backends::ytdlp::json::{
            f64_field, str_field, u32_field, u64_field,
        };

        let format = value.get("format").map(|f| MediaFormatInfo {
            format_name: str_field(f, "format_name"),
            format_long_name: str_field(f, "format_long_name"),
            duration: f64_field(f, "duration"),
            size: u64_field(f, "size"),
            bit_rate: u64_field(f, "bit_rate"),
        });

        let mut streams: Vec<MediaStreamInfo> = Vec::new();
        if let Some(arr) = value.get("streams").and_then(|s| s.as_array()) {
            for s in arr {
                streams.push(MediaStreamInfo {
                    index: u64_field(s, "index"),
                    codec_type: str_field(s, "codec_type"),
                    codec_name: str_field(s, "codec_name"),
                    codec_long_name: str_field(s, "codec_long_name"),
                    profile: str_field(s, "profile"),
                    sample_rate: u32_field(s, "sample_rate"),
                    channels: u32_field(s, "channels"),
                    channel_layout: str_field(s, "channel_layout"),
                    width: u32_field(s, "width"),
                    height: u32_field(s, "height"),
                    r_frame_rate: str_field(s, "r_frame_rate"),
                    avg_frame_rate: str_field(s, "avg_frame_rate"),
                    bit_rate: u64_field(s, "bit_rate"),
                });
            }
        }

        Ok(MediaTechnicalInfo { format, streams })
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn generate_local_thumbnail(
    app: AppHandle,
    file_path: String,
    item_id: String,
) -> Result<String, String> {
    crate::orchestrator::thumbnail::generate_local_thumbnail(&app, &file_path, &item_id).await
}
