/// Auto-podcast generation pipeline (desktop-only).
///
/// Triggered after a YouTube download completes (if auto_generate is enabled) or manually
/// via the `generate_podcast` Tauri command.
///
/// Pipeline steps:
///   1. fetch_transcript — yt-dlp: download YouTube VTT → plain text
///   2. generate_script  — claude CLI: transcript → podcast script (graceful fallback)
///   3. narrate          — edge-tts: script → raw_podcast.mp3 + podcast.vtt
///   4. master_audio     — ffmpeg: raw_podcast.mp3 → mastered podcast.mp3
///   5. save artifacts alongside the original download, update history, emit events

#[cfg(desktop)]
use std::path::{Path, PathBuf};
#[cfg(desktop)]
use std::process::Stdio;
#[cfg(desktop)]
use std::sync::Arc;
#[cfg(desktop)]
use std::time::{Duration, Instant};

#[cfg(desktop)]
use tauri::{AppHandle, Emitter, Manager};
#[cfg(desktop)]
use tokio::time::timeout;
#[cfg(desktop)]
use tracing::{debug, error, info, warn};

#[cfg(desktop)]
use crate::orchestrator::history::HistoryStore;
#[cfg(desktop)]
use crate::orchestrator::types::{HistoryItem, PodcastProgress, PodcastResult, PodcastSettings};

// ── Tool discovery ────────────────────────────────────────────────────────────

#[cfg(desktop)]
fn find_tool(name: &str, app: &AppHandle) -> Option<PathBuf> {
    // 1. Check the podcast venv managed by Comine
    if let Ok(app_data) = app.path().app_data_dir() {
        #[cfg(target_os = "windows")]
        let venv_bin = app_data.join("podcast-venv").join("Scripts").join(name);
        #[cfg(not(target_os = "windows"))]
        let venv_bin = app_data.join("podcast-venv").join("bin").join(name);

        if venv_bin.exists() {
            return Some(venv_bin);
        }
    }

    // 2. Fallback to system PATH
    crate::deps::engine::verify::find_in_system_path(name)
}

#[cfg(desktop)]
fn find_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    crate::deps::resolve_ffmpeg_path(app)
}

// ── VTT → plain text ──────────────────────────────────────────────────────────

#[cfg(desktop)]
fn vtt_to_plain_text(vtt_content: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut prev_line: Option<String> = None;

    let mut in_style_block = false;

    for raw_line in vtt_content.lines() {
        let line = raw_line.trim();

        // Skip WEBVTT header
        if line.starts_with("WEBVTT") {
            continue;
        }

        // Track style blocks
        if line == "STYLE" {
            in_style_block = true;
            continue;
        }
        if in_style_block {
            if line.is_empty() {
                in_style_block = false;
            }
            continue;
        }

        // Skip empty lines
        if line.is_empty() {
            continue;
        }

        // Skip timestamp lines (00:00:00.000 --> 00:00:05.000 or 00:00.000 --> 00:05.000)
        if line.contains("-->") {
            continue;
        }

        // Skip pure numeric cue identifiers
        if line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        // Strip HTML-like tags: <c>, </c>, <00:00:00.000>, etc.
        let cleaned = strip_vtt_tags(line);

        if cleaned.is_empty() {
            continue;
        }

        // Deduplicate consecutive identical lines (yt-dlp auto-subs repeat lines)
        if prev_line.as_deref() == Some(&cleaned) {
            continue;
        }

        prev_line = Some(cleaned.clone());
        lines.push(cleaned);
    }

    lines.join(" ")
}

#[cfg(desktop)]
fn strip_vtt_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside_tag = false;

    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            inside_tag = true;
        } else if chars[i] == '>' {
            inside_tag = false;
        } else if !inside_tag {
            out.push(chars[i]);
        }
        i += 1;
    }

    out.trim().to_string()
}

// ── PodcastPipeline ───────────────────────────────────────────────────────────

#[cfg(desktop)]
pub struct PodcastPipeline {
    app: AppHandle,
    history_id: String,
    video_url: String,
    title: String,
    author: String,
    output_dir: PathBuf,
    settings: PodcastSettings,
    workdir: PathBuf,
}

#[cfg(desktop)]
impl PodcastPipeline {
    pub fn new(
        app: AppHandle,
        history_id: String,
        video_url: String,
        title: String,
        author: String,
        output_dir: PathBuf,
        settings: PodcastSettings,
    ) -> Result<Self, String> {
        // Create a unique temp workdir in the system cache directory
        let cache_base = dirs::cache_dir()
            .map(|d| d.join("comine").join("podcast"))
            .unwrap_or_else(|| std::env::temp_dir().join("comine").join("podcast"));

        let workdir = cache_base.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&workdir)
            .map_err(|e| format!("Failed to create podcast workdir: {}", e))?;

        Ok(Self {
            app,
            history_id,
            video_url,
            title,
            author,
            output_dir,
            settings,
            workdir,
        })
    }

    fn emit_progress(&self, step: &str, progress: f64, error: Option<String>) {
        let _ = self.app.emit(
            "podcast-generation-progress",
            PodcastProgress {
                history_id: self.history_id.clone(),
                step: step.to_string(),
                progress,
                error,
            },
        );
    }

    // ── Step 1: Fetch YouTube transcript via yt-dlp ────────────────────────────

    async fn fetch_transcript(&self) -> Result<PathBuf, String> {
        let ytdlp = crate::deps::resolve_ytdlp_path(&self.app)
            .ok_or_else(|| "yt-dlp not installed. Install it from Settings → Dependencies.".to_string())?;

        // yt-dlp writes files named: {workdir}/subs.{lang}.vtt (or similar)
        let subs_template = self.workdir.join("subs");

        info!(
            history_id = %self.history_id,
            url = %self.video_url,
            lang = %self.settings.subtitle_lang,
            "Podcast step 1/4: fetching YouTube transcript with yt-dlp"
        );
        let t = Instant::now();

        let mut cmd = crate::utils::new_command(&ytdlp);
        cmd.arg("--write-subs")
            .arg("--write-auto-subs")
            .arg("--skip-download")
            .arg("--sub-lang")
            .arg(&self.settings.subtitle_lang)
            .arg("--sub-format")
            .arg("vtt")
            .arg("-o")
            .arg(&subs_template)
            .arg(&self.video_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let result = timeout(Duration::from_secs(30), cmd.output())
            .await
            .map_err(|_| "yt-dlp subtitle download timed out (30s limit)".to_string())?
            .map_err(|e| format!("yt-dlp failed to start: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!(
                "yt-dlp subtitle download failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            ));
        }

        // Find the produced VTT file — yt-dlp appends lang code and extension.
        // Pattern: subs.{lang}.vtt or subs.{lang}-{variant}.vtt
        let vtt_file = self.find_vtt_file()?;

        let vtt_content = tokio::fs::read_to_string(&vtt_file)
            .await
            .map_err(|e| format!("Failed to read VTT subtitle file: {}", e))?;

        let plain_text = vtt_to_plain_text(&vtt_content);

        if plain_text.trim().is_empty() {
            return Err("No YouTube transcript available for this video".to_string());
        }

        let transcript_path = self.workdir.join("transcript.txt");
        tokio::fs::write(&transcript_path, plain_text.as_bytes())
            .await
            .map_err(|e| format!("Failed to write transcript.txt: {}", e))?;

        info!(
            history_id = %self.history_id,
            elapsed_ms = %t.elapsed().as_millis(),
            "Podcast step 1/4: transcript fetched successfully"
        );
        Ok(transcript_path)
    }

    fn find_vtt_file(&self) -> Result<PathBuf, String> {
        let entries = std::fs::read_dir(&self.workdir)
            .map_err(|e| format!("Failed to read workdir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("vtt") {
                return Ok(path);
            }
        }

        Err("No YouTube transcript available for this video".to_string())
    }

    // ── Step 2: Generate podcast script via Claude CLI ────────────────────────

    async fn generate_script(&self, transcript_path: &Path) -> Result<PathBuf, String> {
        let script_path = self.workdir.join("script.txt");

        let claude_path = find_tool("claude", &self.app);
        if claude_path.is_none() {
            warn!(
                history_id = %self.history_id,
                "claude CLI not found — using raw transcript as podcast script. \
                 Install Claude CLI from https://claude.ai/cli for better results."
            );
            // Graceful fallback: copy transcript directly as the script
            std::fs::copy(transcript_path, &script_path)
                .map_err(|e| format!("Failed to copy transcript as fallback script: {}", e))?;
            return Ok(script_path);
        }

        let claude = claude_path.unwrap();

        let prompt = self.settings.claude_prompt.as_deref().unwrap_or(
            "You are a podcast scriptwriter. Given this transcript from a YouTube video, \
             write a concise podcast-style script (2-4 minutes spoken). Focus on the real \
             takeaways and insights, not just paraphrasing. Write it optimized for spoken \
             delivery — short sentences, natural rhythm, clear transitions. Start with a brief \
             hook, cover the key points, end with a takeaway.",
        );

        let full_prompt = format!(
            "{} The video is titled '{}' by {}.",
            prompt, self.title, self.author
        );

        info!(
            history_id = %self.history_id,
            "Podcast step 2/4: generating podcast script via Claude CLI"
        );
        let t = Instant::now();

        let transcript_content = tokio::fs::read_to_string(transcript_path)
            .await
            .map_err(|e| format!("Failed to read transcript: {}", e))?;

        let mut cmd = crate::utils::new_command(&claude);
        // Remove CLAUDECODE env var to avoid "nested session" detection
        cmd.env_remove("CLAUDECODE")
            .arg("--print")
            .arg(&full_prompt)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start claude CLI: {}", e))?;

        // Write transcript to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            if let Err(e) = stdin.write_all(transcript_content.as_bytes()).await {
                warn!(history_id = %self.history_id, "Failed to pipe transcript to claude: {}", e);
            }
            // stdin drops here → EOF sent to claude
        }

        // Claude CLI: 2 minute timeout
        let output = timeout(Duration::from_secs(120), child.wait_with_output())
            .await
            .map_err(|_| "Claude CLI timed out (2 min limit)".to_string())?
            .map_err(|e| format!("Claude CLI error: {}", e))?;

        if !output.status.success() {
            warn!(
                history_id = %self.history_id,
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Claude CLI failed — falling back to raw transcript"
            );
            std::fs::copy(transcript_path, &script_path)
                .map_err(|e| format!("Failed to copy transcript as fallback script: {}", e))?;
            return Ok(script_path);
        }

        let script_text = String::from_utf8_lossy(&output.stdout);
        tokio::fs::write(&script_path, script_text.as_bytes())
            .await
            .map_err(|e| format!("Failed to write script file: {}", e))?;

        info!(
            history_id = %self.history_id,
            elapsed_ms = %t.elapsed().as_millis(),
            "Podcast step 2/4: script generation complete"
        );
        Ok(script_path)
    }

    // ── Step 3: TTS narration with edge-tts ──────────────────────────────────

    async fn narrate(&self, script_path: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
        let edge_tts = find_tool("edge-tts", &self.app).ok_or_else(|| {
            "edge-tts not found — install with: pip install edge-tts\n\
             Or enable the podcast venv in Settings → Podcast."
                .to_string()
        })?;

        let raw_mp3_path = self.workdir.join("raw_podcast.mp3");
        let vtt_path = self.workdir.join("podcast.vtt");

        info!(
            history_id = %self.history_id,
            voice = %self.settings.tts_voice,
            rate = %self.settings.tts_rate,
            "Podcast step 3/4: narrating with edge-tts"
        );
        let t = Instant::now();

        let script_content = tokio::fs::read_to_string(script_path)
            .await
            .map_err(|e| format!("Failed to read script: {}", e))?;

        let mut cmd = crate::utils::new_command(&edge_tts);
        cmd.arg("--voice")
            .arg(&self.settings.tts_voice)
            .arg("--rate")
            .arg(&self.settings.tts_rate)
            .arg("--text")
            .arg(&script_content)
            .arg("--write-media")
            .arg(&raw_mp3_path)
            .arg("--write-subtitles")
            .arg(&vtt_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        // edge-tts: 2 minute timeout
        let result = timeout(Duration::from_secs(120), cmd.output())
            .await
            .map_err(|_| "edge-tts timed out (2 min limit)".to_string())?
            .map_err(|e| format!("edge-tts failed to start: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!(
                "edge-tts failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            ));
        }

        if !raw_mp3_path.exists() {
            return Err("edge-tts did not produce the expected MP3 file".to_string());
        }

        let vtt = if vtt_path.exists() {
            Some(vtt_path)
        } else {
            None
        };

        info!(
            history_id = %self.history_id,
            elapsed_ms = %t.elapsed().as_millis(),
            "Podcast step 3/4: narration complete"
        );
        Ok((raw_mp3_path, vtt))
    }

    // ── Step 4: FFmpeg audio mastering ────────────────────────────────────────

    async fn master_audio(&self, raw_mp3: &Path, output_path: &Path) -> Result<(), String> {
        let ffmpeg = find_ffmpeg(&self.app).ok_or_else(|| {
            "FFmpeg not installed. Install it from Settings → Dependencies.".to_string()
        })?;

        info!(
            history_id = %self.history_id,
            output = %output_path.display(),
            "Podcast step 4/4: mastering audio with FFmpeg"
        );
        let t = Instant::now();

        let mut cmd = crate::utils::new_command(&ffmpeg);
        cmd.arg("-y")
            .arg("-i")
            .arg(raw_mp3)
            .args([
                "-af",
                "highpass=f=80,lowpass=f=12000,acompressor=threshold=-18dB:ratio=3:attack=5:release=50,loudnorm=I=-16:TP=-1.5:LRA=11",
                "-ar",
                "44100",
                "-b:a",
                "192k",
            ])
            .arg(output_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        // FFmpeg mastering: 1 minute timeout
        let result = timeout(Duration::from_secs(60), cmd.output())
            .await
            .map_err(|_| "FFmpeg mastering timed out (1 min limit)".to_string())?
            .map_err(|e| format!("FFmpeg mastering failed to start: {}", e))?;

        if !result.status.success() || !output_path.exists() {
            warn!(
                history_id = %self.history_id,
                "FFmpeg mastering failed — using raw TTS output as fallback"
            );
            // Graceful fallback: copy unmastered version
            tokio::fs::copy(raw_mp3, output_path)
                .await
                .map_err(|e| format!("Failed to copy raw podcast as fallback: {}", e))?;
        }

        info!(
            history_id = %self.history_id,
            elapsed_ms = %t.elapsed().as_millis(),
            "Podcast step 4/4: mastering complete"
        );
        Ok(())
    }

    // ── Orchestrator ──────────────────────────────────────────────────────────

    pub async fn run(self) -> Result<PodcastResult, String> {
        let history_id = self.history_id.clone();

        // Sanitize title for filesystem use
        let safe_title: String = self
            .title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim()
            .chars()
            .take(80)
            .collect();
        let safe_title = if safe_title.is_empty() {
            "podcast".to_string()
        } else {
            safe_title
        };

        // Destinations in output_dir (alongside the original download)
        let podcast_mp3 = self.output_dir.join(format!("{}_podcast.mp3", safe_title));
        let podcast_vtt = self.output_dir.join(format!("{}_podcast.vtt", safe_title));
        let transcript_dest = self.output_dir.join(format!("{}_transcript.txt", safe_title));
        let script_dest = self.output_dir.join(format!("{}_script.txt", safe_title));

        // ── 1. Fetch YouTube transcript ───────────────────────────────────────
        self.emit_progress("fetching_transcript", 0.1, None);
        let transcript_path = match self.fetch_transcript().await {
            Ok(p) => p,
            Err(e) => {
                self.emit_progress("failed", 0.0, Some(e.clone()));
                self.cleanup();
                return Err(e);
            }
        };

        // ── 2. Generate script ────────────────────────────────────────────────
        self.emit_progress("generating_script", 0.35, None);
        let script_path = match self.generate_script(&transcript_path).await {
            Ok(p) => p,
            Err(e) => {
                self.emit_progress("failed", 0.0, Some(e.clone()));
                self.cleanup();
                return Err(e);
            }
        };

        // ── 3. Narrate ────────────────────────────────────────────────────────
        self.emit_progress("narrating", 0.55, None);
        let (raw_mp3, vtt_path) = match self.narrate(&script_path).await {
            Ok(r) => r,
            Err(e) => {
                self.emit_progress("failed", 0.0, Some(e.clone()));
                self.cleanup();
                return Err(e);
            }
        };

        // ── 4. Master audio ───────────────────────────────────────────────────
        self.emit_progress("mastering", 0.8, None);
        if self.settings.mastering_enabled {
            if let Err(e) = self.master_audio(&raw_mp3, &podcast_mp3).await {
                self.emit_progress("failed", 0.0, Some(e.clone()));
                self.cleanup();
                return Err(e);
            }
        } else {
            // Skip mastering: copy raw directly to destination
            if let Err(e) = tokio::fs::copy(&raw_mp3, &podcast_mp3).await {
                let msg = format!("Failed to copy raw podcast: {}", e);
                self.emit_progress("failed", 0.0, Some(msg.clone()));
                self.cleanup();
                return Err(msg);
            }
        }

        // ── 5. Copy artifacts to output dir ──────────────────────────────────
        let _ = tokio::fs::copy(&transcript_path, &transcript_dest).await;
        let _ = tokio::fs::copy(&script_path, &script_dest).await;

        // VTT subtitle (if produced)
        let final_vtt = if let Some(ref vp) = vtt_path {
            match tokio::fs::copy(vp, &podcast_vtt).await {
                Ok(_) => Some(podcast_vtt.to_string_lossy().to_string()),
                Err(e) => {
                    warn!(history_id = %history_id, "Failed to copy VTT: {}", e);
                    None
                }
            }
        } else {
            None
        };

        self.cleanup();

        self.emit_progress("complete", 1.0, None);
        let _ = self.app.emit(
            "podcast-generation-completed",
            serde_json::json!({
                "historyId": history_id,
                "podcastPath": podcast_mp3.to_string_lossy(),
                "subtitlePath": final_vtt,
            }),
        );

        info!(
            history_id = %history_id,
            podcast = %podcast_mp3.display(),
            "Podcast generation complete"
        );

        Ok(PodcastResult {
            podcast_path: podcast_mp3.to_string_lossy().to_string(),
            subtitle_path: final_vtt,
            transcript_path: transcript_dest.to_string_lossy().to_string(),
            script_path: script_dest.to_string_lossy().to_string(),
        })
    }

    fn cleanup(&self) {
        if let Err(e) = std::fs::remove_dir_all(&self.workdir) {
            debug!("Failed to clean up podcast workdir: {}", e);
        }
    }
}

// ── Settings I/O ──────────────────────────────────────────────────────────────

#[cfg(desktop)]
pub fn load_podcast_settings(app: &AppHandle) -> PodcastSettings {
    use tauri_plugin_store::StoreExt;
    let store = match app.store("settings.json") {
        Ok(s) => s,
        Err(_) => return PodcastSettings::default(),
    };

    let raw = match store.get("podcastSettings") {
        Some(v) => v,
        None => return PodcastSettings::default(),
    };

    match serde_json::from_value::<PodcastSettings>(raw.clone()) {
        Ok(s) => {
            debug!(
                enabled = s.enabled,
                auto_generate = s.auto_generate,
                "Loaded podcast settings"
            );
            s
        }
        Err(e) => {
            warn!("Failed to deserialize podcast settings (using defaults): {}", e);
            debug!("Raw podcast settings value: {:?}", raw);
            PodcastSettings::default()
        }
    }
}

#[cfg(desktop)]
pub fn save_podcast_settings(app: &AppHandle, settings: &PodcastSettings) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings store: {}", e))?;
    let value = serde_json::to_value(settings)
        .map_err(|e| format!("Failed to serialize podcast settings: {}", e))?;
    store.set("podcastSettings", value);
    Ok(())
}

// ── Auto-trigger helper ───────────────────────────────────────────────────────

/// Called from `manager.rs` `complete_job()` to optionally auto-start the podcast pipeline.
/// This is a no-op on mobile.
#[cfg(desktop)]
pub async fn maybe_auto_generate_podcast(
    app: AppHandle,
    history: Arc<HistoryStore>,
    item: HistoryItem,
) {
    let settings = load_podcast_settings(&app);

    if !settings.enabled || !settings.auto_generate {
        debug!(
            url = %item.url,
            enabled = settings.enabled,
            auto_generate = settings.auto_generate,
            "Podcast auto-generate skipped: feature disabled"
        );
        return;
    }

    // Only for YouTube URLs
    let is_youtube = item.url.contains("youtube.com") || item.url.contains("youtu.be");
    if !is_youtube {
        debug!(url = %item.url, "Podcast auto-generate skipped: not a YouTube URL");
        return;
    }

    // Don't generate for directories or galleries
    if item.is_directory {
        debug!(url = %item.url, "Podcast auto-generate skipped: directory download");
        return;
    }

    info!(
        history_id = %item.id,
        url = %item.url,
        title = %item.title,
        "Podcast auto-generate: starting pipeline for YouTube download"
    );

    let history_id = item.id.clone();
    let video_url = item.url.clone();
    let title = item.title.clone();
    let author = item.author.clone();

    let output_dir = PathBuf::from(&item.file_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Mark as generating in DB
    history
        .update_podcast_status(&history_id, "generating", None, None)
        .await;

    let _ = app.emit(
        "podcast-generation-started",
        serde_json::json!({ "historyId": history_id }),
    );

    let app_clone = app.clone();
    let history_clone = history.clone();
    tokio::spawn(async move {
        let pipeline = match PodcastPipeline::new(
            app_clone.clone(),
            history_id.clone(),
            video_url,
            title,
            author,
            output_dir,
            settings,
        ) {
            Ok(p) => p,
            Err(e) => {
                error!(history_id = %history_id, "Failed to create podcast pipeline: {}", e);
                history_clone
                    .update_podcast_status(&history_id, "failed", None, None)
                    .await;
                let _ = app_clone.emit(
                    "podcast-generation-failed",
                    serde_json::json!({ "historyId": history_id, "error": e }),
                );
                return;
            }
        };

        match pipeline.run().await {
            Ok(result) => {
                history_clone
                    .update_podcast_status(
                        &history_id,
                        "completed",
                        Some(&result.podcast_path),
                        result.subtitle_path.as_deref(),
                    )
                    .await;
            }
            Err(e) => {
                error!(history_id = %history_id, "Podcast generation failed: {}", e);
                history_clone
                    .update_podcast_status(&history_id, "failed", None, None)
                    .await;
                let _ = app_clone.emit(
                    "podcast-generation-failed",
                    serde_json::json!({ "historyId": history_id, "error": e }),
                );
            }
        }
    });
}

/// No-op on mobile — podcast pipeline requires desktop tooling.
#[cfg(mobile)]
pub async fn maybe_auto_generate_podcast(
    _app: tauri::AppHandle,
    _history: std::sync::Arc<crate::orchestrator::history::HistoryStore>,
    _item: crate::orchestrator::types::HistoryItem,
) {
}

// ── Tauri Commands ────────────────────────────────────────────────────────────

/// Manually trigger podcast generation for a history item.
///
/// The command returns immediately; track progress via `podcast-generation-progress`,
/// `podcast-generation-completed`, and `podcast-generation-failed` events.
#[cfg(desktop)]
#[tauri::command]
pub async fn generate_podcast(
    app: AppHandle,
    state: tauri::State<'_, Arc<crate::orchestrator::manager::JobManager>>,
    history_id: String,
) -> Result<(), String> {
    let settings = load_podcast_settings(&app);

    if !settings.enabled {
        return Err(
            "Podcast generation is disabled. Enable it in Settings → Podcast.".to_string(),
        );
    }

    let item = state
        .history
        .get_by_id(&history_id)
        .await
        .ok_or_else(|| format!("History item not found: {}", history_id))?;

    let source_path = PathBuf::from(&item.file_path);
    if !source_path.exists() {
        return Err(format!(
            "Source file not found: {}. The file may have been moved or deleted.",
            source_path.display()
        ));
    }

    let output_dir = source_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Mark as generating
    state
        .history
        .update_podcast_status(&history_id, "generating", None, None)
        .await;

    let _ = app.emit(
        "podcast-generation-started",
        serde_json::json!({ "historyId": history_id }),
    );

    let pipeline = PodcastPipeline::new(
        app.clone(),
        history_id.clone(),
        item.url.clone(),
        item.title.clone(),
        item.author.clone(),
        output_dir,
        settings,
    )?;

    let history_clone = state.history.clone();
    let app_clone = app.clone();
    let history_id_clone = history_id.clone();

    tokio::spawn(async move {
        match pipeline.run().await {
            Ok(result) => {
                history_clone
                    .update_podcast_status(
                        &history_id_clone,
                        "completed",
                        Some(&result.podcast_path),
                        result.subtitle_path.as_deref(),
                    )
                    .await;
            }
            Err(e) => {
                error!(history_id = %history_id_clone, "Podcast generation failed: {}", e);
                history_clone
                    .update_podcast_status(&history_id_clone, "failed", None, None)
                    .await;
                let _ = app_clone.emit(
                    "podcast-generation-failed",
                    serde_json::json!({ "historyId": history_id_clone, "error": e }),
                );
            }
        }
    });

    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn get_podcast_settings(app: AppHandle) -> Result<PodcastSettings, String> {
    Ok(load_podcast_settings(&app))
}

#[cfg(desktop)]
#[tauri::command]
pub async fn set_podcast_settings(
    app: AppHandle,
    settings: PodcastSettings,
) -> Result<(), String> {
    save_podcast_settings(&app, &settings)
}
