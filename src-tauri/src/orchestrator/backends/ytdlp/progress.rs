use tracing::{debug, info};

use crate::orchestrator::backends::parse_size_str;
use crate::orchestrator::types::ProgressUpdate;

pub struct RawProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<u64>,
    pub eta: Option<u64>,
    pub filename: Option<String>,
}

pub fn extract_raw_progress(line: &str) -> Option<RawProgress> {
    let start = line.find("__COMINE_PROGRESS__")?;
    let end = line.rfind("__COMINE_PROGRESS__")?;
    if start >= end {
        return None;
    }

    let content = &line[start + 19..end];

    let extract_value = |key: &str| -> Option<&str> {
        let pattern = format!("{}:", key);
        let start_idx = content.find(&pattern)?;
        let value_start = start_idx + pattern.len();
        let rest = &content[value_start..];
        // filename field can contain commas, so only split on '}'
        if key == "filename" {
            let end_idx = rest.find('}').unwrap_or(rest.len());
            Some(rest[..end_idx].trim())
        } else {
            let end_idx = rest.find([',', '}']).unwrap_or(rest.len());
            Some(rest[..end_idx].trim())
        }
    };

    let parse_u64 = |val: Option<&str>| -> Option<u64> {
        let s = val?;
        if s == "NA" || s.is_empty() {
            None
        } else {
            s.parse::<f64>().ok().map(|n| n as u64)
        }
    };

    let downloaded = parse_u64(extract_value("downloaded")).unwrap_or(0);
    let total =
        parse_u64(extract_value("total")).or_else(|| parse_u64(extract_value("total_estimate")));
    let speed = parse_u64(extract_value("speed"));
    let eta = parse_u64(extract_value("eta"));
    let filename = extract_value("filename").and_then(|f| {
        if f == "NA" || f.is_empty() {
            None
        } else {
            Some(f.to_string())
        }
    });

    Some(RawProgress {
        downloaded,
        total,
        speed,
        eta,
        filename,
    })
}

pub struct ProgressTracker {
    job_id: String,
    completed_bytes: u64,
    completed_total: u64,
    current_filename: Option<String>,
    current_file_total: Option<u64>,
    current_phase_max_downloaded: u64,
    last_emitted_cumulative: u64,
    section_durations: Vec<f64>,
    section_starts: Vec<f64>,
    total_duration: f64,
    completed_duration: f64,
    current_section_idx: usize,
    captured_files: Vec<String>,
}

impl ProgressTracker {
    pub fn new(job_id: String) -> Self {
        Self {
            job_id,
            completed_bytes: 0,
            completed_total: 0,
            current_filename: None,
            current_file_total: None,
            current_phase_max_downloaded: 0,
            last_emitted_cumulative: 0,
            section_durations: Vec::new(),
            section_starts: Vec::new(),
            total_duration: 0.0,
            completed_duration: 0.0,
            current_section_idx: 0,
            captured_files: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn set_section_durations(&mut self, durations: Vec<f64>) {
        self.total_duration = durations.iter().sum();
        self.section_durations = durations;
        self.section_starts = Vec::new();
        self.completed_duration = 0.0;
        self.current_section_idx = 0;
        info!(target: "ytdlp", "Set section durations: {:?}, total: {:.1}s", self.section_durations, self.total_duration);
    }

    /// Set section durations with start times for matching by filename timestamp.
    pub fn set_section_info(&mut self, durations: Vec<f64>, starts: Vec<f64>) {
        self.total_duration = durations.iter().sum();
        self.section_starts = starts;
        self.section_durations = durations;
        self.completed_duration = 0.0;
        self.current_section_idx = 0;
        info!(target: "ytdlp", "Set section durations: {:?}, starts: {:?}, total: {:.1}s",
            self.section_durations, self.section_starts, self.total_duration);
    }

    /// Match section index by parsing `_<start>-<end>` from filename.
    fn find_section_by_filename(&self, filename: &str) -> Option<usize> {
        if self.section_starts.is_empty() {
            return None;
        }
        let stem = std::path::Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())?;
        let base = stem
            .rsplit_once('.')
            .and_then(|(b, suffix)| {
                if suffix.starts_with('f') && suffix[1..].parse::<u32>().is_ok() {
                    Some(b)
                } else {
                    None
                }
            })
            .unwrap_or(stem);
        let last_underscore = base.rfind('_')?;
        let section_part = &base[last_underscore + 1..];
        let dash_pos = section_part.find('-')?;
        let start_str = &section_part[..dash_pos];
        let start: f64 = start_str.parse().ok()?;
        self.section_starts
            .iter()
            .position(|&s| (s - start).abs() < 0.5)
    }

    #[allow(dead_code)]
    pub fn captured_files(&self) -> Vec<String> {
        let mut files = self.captured_files.clone();
        if let Some(ref current) = self.current_filename {
            if !current.is_empty() && !files.contains(current) {
                files.push(current.clone());
            }
        }
        files
    }

    pub fn parse_progress_line(&mut self, line: &str) -> Option<ProgressUpdate> {
        let raw = extract_raw_progress(line)?;

        let (cumulative_downloaded, cumulative_total) =
            self.update_multi_phase(raw.downloaded, raw.total, raw.filename.as_deref());

        debug!(target: "ytdlp", "Progress: file={:?}, downloaded={}, total={:?}, cumulative={}/{:?}",
               raw.filename, raw.downloaded, raw.total, cumulative_downloaded, cumulative_total);

        Some(ProgressUpdate {
            job_id: self.job_id.clone(),
            downloaded_bytes: cumulative_downloaded,
            total_bytes: cumulative_total,
            speed: raw.speed,
            eta: raw.eta,
        })
    }

    /// Feed raw progress from an external downloader (aria2c) through section scaling.
    /// No filename, so no phase/section transitions are triggered.
    pub fn feed_external_progress(
        &mut self,
        downloaded: u64,
        total: Option<u64>,
        speed: Option<u64>,
        eta: Option<u64>,
    ) -> Option<ProgressUpdate> {
        if let Some(t) = total {
            if t > 0 {
                self.current_file_total = Some(t);
            }
        }

        self.current_phase_max_downloaded = self.current_phase_max_downloaded.max(downloaded);

        if !self.section_durations.is_empty() && self.total_duration > 0.0 {
            let section_progress = if let Some(file_total) = self.current_file_total {
                if file_total > 0 {
                    (self.current_phase_max_downloaded as f64) / (file_total as f64)
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let current_section_partial = self.current_section_duration() * section_progress;
            let total_progress = self.completed_duration + current_section_partial;
            let progress_ratio = (total_progress / self.total_duration).min(1.0);

            let fake_total = 1_000_000u64;
            let fake_downloaded = (progress_ratio * fake_total as f64) as u64;

            let fake_downloaded = fake_downloaded.max(self.last_emitted_cumulative);
            self.last_emitted_cumulative = fake_downloaded;

            Some(ProgressUpdate {
                job_id: self.job_id.clone(),
                downloaded_bytes: fake_downloaded,
                total_bytes: Some(fake_total),
                speed,
                eta,
            })
        } else {
            let base_cumulative = self.completed_bytes + self.current_phase_max_downloaded;
            let cumulative_downloaded = base_cumulative.max(self.last_emitted_cumulative);
            self.last_emitted_cumulative = cumulative_downloaded;

            let cumulative_total =
                if self.current_phase_max_downloaded == 0 && self.completed_total > 0 {
                    Some(self.completed_total)
                } else {
                    self.current_file_total.map(|t| self.completed_total + t)
                };

            Some(ProgressUpdate {
                job_id: self.job_id.clone(),
                downloaded_bytes: cumulative_downloaded,
                total_bytes: cumulative_total,
                speed,
                eta,
            })
        }
    }

    fn current_section_duration(&self) -> f64 {
        self.section_durations
            .get(self.current_section_idx)
            .copied()
            .unwrap_or(0.0)
    }

    fn update_multi_phase(
        &mut self,
        downloaded: u64,
        total: Option<u64>,
        filename: Option<&str>,
    ) -> (u64, Option<u64>) {
        let filename_changed = match (&self.current_filename, filename) {
            (Some(current), Some(new)) => current != new,
            (None, Some(_)) => false,
            _ => false,
        };

        if filename_changed {
            if let Some(ref completed_file) = self.current_filename {
                if !completed_file.is_empty() && !self.captured_files.contains(completed_file) {
                    self.captured_files.push(completed_file.clone());
                    info!(target: "ytdlp", "Captured completed file: {}", completed_file);
                }
            }

            if let Some(prev_total) = self.current_file_total {
                self.completed_bytes += prev_total;
                self.completed_total += prev_total;
                info!(target: "ytdlp", "Phase transition: completed {} bytes, new file: {:?}", prev_total, filename);
            } else {
                self.completed_bytes += self.current_phase_max_downloaded;
                self.completed_total += self.current_phase_max_downloaded;
                info!(target: "ytdlp", "Phase transition (estimated): completed {} bytes, new file: {:?}", self.current_phase_max_downloaded, filename);
            }
            self.current_phase_max_downloaded = 0;
            self.current_file_total = None;

            if !self.section_durations.is_empty() {
                let is_section_boundary = if let (Some(ref current), Some(new_file)) =
                    (&self.current_filename, filename)
                {
                    let cur_stem = std::path::Path::new(current)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(current);
                    let new_stem = std::path::Path::new(new_file)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(new_file);
                    let cur_base = cur_stem
                        .rsplit_once('.')
                        .map(|(b, _)| b)
                        .unwrap_or(cur_stem);
                    let new_base = new_stem
                        .rsplit_once('.')
                        .map(|(b, _)| b)
                        .unwrap_or(new_stem);
                    cur_base != new_base
                } else {
                    false
                };

                if is_section_boundary {
                    // Match by filename timestamp to handle skipped sections
                    if let Some(matched_idx) =
                        filename.and_then(|f| self.find_section_by_filename(f))
                    {
                        self.completed_duration =
                            self.section_durations[..matched_idx].iter().sum();
                        self.current_section_idx = matched_idx;
                    } else {
                        self.completed_duration += self.current_section_duration();
                        self.current_section_idx =
                            (self.current_section_idx + 1).min(self.section_durations.len() - 1);
                    }
                    info!(target: "ytdlp", "Section transition: completed {:.1}s, now on section {}/{} ({:.1}s)",
                        self.completed_duration,
                        self.current_section_idx + 1,
                        self.section_durations.len(),
                        self.current_section_duration());
                }
            }
        }

        if let Some(f) = filename {
            self.current_filename = Some(f.to_string());
        }

        if let Some(t) = total {
            self.current_file_total = Some(t);
        }

        self.current_phase_max_downloaded = self.current_phase_max_downloaded.max(downloaded);

        if !self.section_durations.is_empty() && self.total_duration > 0.0 {
            let section_progress = if let Some(total) = self.current_file_total {
                if total > 0 {
                    (self.current_phase_max_downloaded as f64) / (total as f64)
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let current_section_partial = self.current_section_duration() * section_progress;
            let total_progress = self.completed_duration + current_section_partial;
            let progress_ratio = (total_progress / self.total_duration).min(1.0);

            let fake_total = 1_000_000u64;
            let fake_downloaded = (progress_ratio * fake_total as f64) as u64;

            let fake_downloaded = fake_downloaded.max(self.last_emitted_cumulative);
            self.last_emitted_cumulative = fake_downloaded;

            (fake_downloaded, Some(fake_total))
        } else {
            let base_cumulative = self.completed_bytes + self.current_phase_max_downloaded;
            if base_cumulative >= self.last_emitted_cumulative {
                self.last_emitted_cumulative = base_cumulative;
            }

            // Defer increasing total until new file has progress to avoid % regression
            let cumulative_total =
                if self.current_phase_max_downloaded == 0 && self.completed_total > 0 {
                    Some(self.completed_total)
                } else {
                    self.current_file_total.map(|t| self.completed_total + t)
                };

            (self.last_emitted_cumulative, cumulative_total)
        }
    }
}

#[allow(dead_code)]
pub fn parse_ffmpeg_progress_line(line: &str, job_id: &str) -> Option<ProgressUpdate> {
    if !line.contains("time=") || !line.contains("size=") {
        return None;
    }

    let mut downloaded = 0;
    let mut speed = None;

    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts {
        if let Some(s) = part.strip_prefix("size=") {
            if s != "N/A" {
                downloaded = parse_size_str(s).unwrap_or(0);
            }
        } else if let Some(s) = part.strip_prefix("bitrate=") {
            let s_clean = s.trim_end_matches("kbits/s").trim_end_matches("bits/s");
            if let Ok(val) = s_clean.parse::<f64>() {
                speed = Some((val * 1000.0 / 8.0) as u64);
            }
        }
    }

    if downloaded > 0 {
        Some(ProgressUpdate {
            job_id: job_id.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: None,
            speed,
            eta: None,
        })
    } else {
        None
    }
}

/// Stateless progress extraction for Android notification updates.
#[cfg(target_os = "android")]
pub fn parse_raw_progress(line: &str, job_id: &str) -> Option<ProgressUpdate> {
    let raw = extract_raw_progress(line)?;

    Some(ProgressUpdate {
        job_id: job_id.to_string(),
        downloaded_bytes: raw.downloaded,
        total_bytes: raw.total,
        speed: raw.speed,
        eta: raw.eta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_extract_raw_basic() {
        let line = "__COMINE_PROGRESS__downloaded:1024,total:2048,speed:512,eta:2,filename:video.mp4__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 1024);
        assert_eq!(raw.total, Some(2048));
        assert_eq!(raw.speed, Some(512));
        assert_eq!(raw.eta, Some(2));
        assert_eq!(raw.filename, Some("video.mp4".to_string()));
    }

    #[test]
    fn test_extract_raw_na_values() {
        let line = "__COMINE_PROGRESS__downloaded:5000,total:NA,speed:NA,eta:NA,filename:NA__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 5000);
        assert_eq!(raw.total, None);
        assert_eq!(raw.speed, None);
        assert_eq!(raw.eta, None);
        assert_eq!(raw.filename, None);
    }

    #[test]
    fn test_extract_raw_total_estimate_fallback() {
        let line = "__COMINE_PROGRESS__downloaded:100,total:NA,total_estimate:500,speed:50,eta:8,filename:test.mp3__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.total, Some(500));
    }

    #[test]
    fn test_extract_raw_total_preferred_over_estimate() {
        let line = "__COMINE_PROGRESS__downloaded:100,total:400,total_estimate:500,speed:50,eta:8,filename:test.mp3__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.total, Some(400));
    }

    #[test]
    fn test_extract_raw_no_markers() {
        assert!(extract_raw_progress("just a regular line").is_none());
    }

    #[test]
    fn test_extract_raw_single_marker() {
        assert!(extract_raw_progress("__COMINE_PROGRESS__").is_none());
    }

    #[test]
    fn test_extract_raw_embedded_in_output() {
        let line = "[download]  50% __COMINE_PROGRESS__downloaded:2048,total:4096,speed:1024,eta:2,filename:test.mp4__COMINE_PROGRESS__ more text";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 2048);
        assert_eq!(raw.total, Some(4096));
    }

    #[test]
    fn test_extract_raw_float_values() {
        let line = "__COMINE_PROGRESS__downloaded:1024.5,total:2048.7,speed:512.3,eta:2.1,filename:test.mp4__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 1024);
        let line = "__COMINE_PROGRESS__downloaded:,total:NA,speed:NA,eta:NA,filename:NA__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 0);
    }

    #[test]
    fn test_extract_raw_large_values() {
        let line = "__COMINE_PROGRESS__downloaded:4294967296,total:8589934592,speed:104857600,eta:40,filename:large.mkv__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 4_294_967_296);
        assert_eq!(raw.total, Some(8_589_934_592));
        assert_eq!(raw.speed, Some(104_857_600));
    }

    #[test]
    fn test_basic_progress_line() {
        let mut tracker = ProgressTracker::new("test_basic".to_string());
        let line = "__COMINE_PROGRESS__downloaded:1024,total:2048,speed:512,eta:2,filename:video.mp4__COMINE_PROGRESS__";
        let result = tracker.parse_progress_line(line).unwrap();
        assert_eq!(result.downloaded_bytes, 1024);
        assert_eq!(result.total_bytes, Some(2048));
        assert_eq!(result.speed, Some(512));
        assert_eq!(result.eta, Some(2));
    }

    #[test]
    fn test_progress_na_values() {
        let mut tracker = ProgressTracker::new("test_na".to_string());
        let line = "__COMINE_PROGRESS__downloaded:5000,total:NA,speed:NA,eta:NA,filename:NA__COMINE_PROGRESS__";
        let result = tracker.parse_progress_line(line).unwrap();
        assert_eq!(result.downloaded_bytes, 5000);
        assert_eq!(result.total_bytes, None);
        assert_eq!(result.speed, None);
        assert_eq!(result.eta, None);
    }

    #[test]
    fn test_progress_with_total_estimate() {
        let mut tracker = ProgressTracker::new("test_est".to_string());
        let line = "__COMINE_PROGRESS__downloaded:100,total:NA,total_estimate:500,speed:50,eta:8,filename:test.mp3__COMINE_PROGRESS__";
        let result = tracker.parse_progress_line(line).unwrap();
        assert_eq!(result.downloaded_bytes, 100);
        assert_eq!(result.total_bytes, Some(500));
    }

    #[test]
    fn test_progress_embedded_in_output() {
        let mut tracker = ProgressTracker::new("test_embed".to_string());
        let line = "[download]  50% __COMINE_PROGRESS__downloaded:2048,total:4096,speed:1024,eta:2,filename:test.mp4__COMINE_PROGRESS__ more text";
        let result = tracker.parse_progress_line(line).unwrap();
        assert_eq!(result.downloaded_bytes, 2048);
        assert_eq!(result.total_bytes, Some(4096));
    }

    #[test]
    fn test_progress_no_markers() {
        let mut tracker = ProgressTracker::new("test_none".to_string());
        assert!(tracker.parse_progress_line("just a regular line").is_none());
    }

    #[test]
    fn test_progress_single_marker() {
        let mut tracker = ProgressTracker::new("test_single".to_string());
        assert!(tracker.parse_progress_line("__COMINE_PROGRESS__").is_none());
    }

    #[test]
    fn test_progress_float_values() {
        let mut tracker = ProgressTracker::new("test_float".to_string());
        let line = "__COMINE_PROGRESS__downloaded:1024.5,total:2048.7,speed:512.3,eta:2.1,filename:test.mp4__COMINE_PROGRESS__";
        let result = tracker.parse_progress_line(line).unwrap();
        assert_eq!(result.downloaded_bytes, 1024);
        assert_eq!(result.total_bytes, Some(2048));
    }

    #[test]
    fn test_multi_phase_progress() {
        let mut tracker = ProgressTracker::new("test_multi_phase".to_string());

        let line1 = "__COMINE_PROGRESS__downloaded:500,total:1000,speed:100,eta:5,filename:video.f137.mp4__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(line1).unwrap();
        assert_eq!(r1.downloaded_bytes, 500);
        assert_eq!(r1.total_bytes, Some(1000));

        let line2 = "__COMINE_PROGRESS__downloaded:1000,total:1000,speed:100,eta:0,filename:video.f137.mp4__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(line2).unwrap();
        assert_eq!(r2.downloaded_bytes, 1000);

        let line3 = "__COMINE_PROGRESS__downloaded:200,total:500,speed:100,eta:3,filename:audio.f251.webm__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(line3).unwrap();
        assert_eq!(r3.downloaded_bytes, 1200);
        assert_eq!(r3.total_bytes, Some(1500));
    }

    #[test]
    fn test_multi_phase_three_files() {
        let mut tracker = ProgressTracker::new("test_3phase".to_string());

        let l1 = "__COMINE_PROGRESS__downloaded:2000,total:2000,speed:500,eta:0,filename:v.f137.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__downloaded:500,total:500,speed:500,eta:0,filename:a.f251.webm__COMINE_PROGRESS__";
        tracker.parse_progress_line(l2);

        let l3 = "__COMINE_PROGRESS__downloaded:10,total:50,speed:100,eta:0,filename:s.en.vtt__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(l3).unwrap();

        assert_eq!(r3.downloaded_bytes, 2510);
        assert_eq!(r3.total_bytes, Some(2550));
    }

    #[test]
    fn test_multi_phase_no_total_on_previous() {
        let mut tracker = ProgressTracker::new("test_no_total".to_string());

        let l1 = "__COMINE_PROGRESS__downloaded:800,total:NA,speed:100,eta:NA,filename:file_a.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__downloaded:200,total:500,speed:100,eta:3,filename:file_b.webm__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        assert_eq!(r2.downloaded_bytes, 1000);
        assert_eq!(r2.total_bytes, Some(1300));
    }

    #[test]
    fn test_progress_same_filename_no_transition() {
        let mut tracker = ProgressTracker::new("test_same".to_string());

        let l1 = "__COMINE_PROGRESS__downloaded:500,total:2000,speed:100,eta:15,filename:video.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__downloaded:1000,total:2000,speed:200,eta:5,filename:video.mp4__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        assert_eq!(r2.downloaded_bytes, 1000);
        assert_eq!(r2.total_bytes, Some(2000));
    }

    #[test]
    fn test_progress_monotonically_increasing() {
        let mut tracker = ProgressTracker::new("test_mono".to_string());

        let l1 = "__COMINE_PROGRESS__downloaded:500,total:1000,speed:100,eta:5,filename:f.mp4__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();
        assert_eq!(r1.downloaded_bytes, 500);

        let l2 = "__COMINE_PROGRESS__downloaded:300,total:1000,speed:100,eta:7,filename:f.mp4__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        assert_eq!(r2.downloaded_bytes, 500);
    }

    #[test]
    fn test_captured_files() {
        let mut tracker = ProgressTracker::new("test_capture".to_string());

        let line1 = "__COMINE_PROGRESS__downloaded:1000,total:1000,speed:100,eta:0,filename:video.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(line1);

        let line2 = "__COMINE_PROGRESS__downloaded:500,total:500,speed:100,eta:0,filename:audio.webm__COMINE_PROGRESS__";
        tracker.parse_progress_line(line2);

        let files = tracker.captured_files();
        assert!(files.contains(&"video.mp4".to_string()));
        assert!(files.contains(&"audio.webm".to_string()));
    }

    #[test]
    fn test_captured_files_no_duplicates() {
        let mut tracker = ProgressTracker::new("test_dedup".to_string());

        for i in 0..5 {
            let line = format!("__COMINE_PROGRESS__downloaded:{},total:1000,speed:100,eta:5,filename:video.mp4__COMINE_PROGRESS__", i * 200);
            tracker.parse_progress_line(&line);
        }

        let files = tracker.captured_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "video.mp4");
    }

    #[test]
    fn test_section_duration_basic() {
        let mut tracker = ProgressTracker::new("test_section".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let l1 = "__COMINE_PROGRESS__downloaded:500,total:1000,speed:100,eta:5,filename:section1.mp4__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();

        assert!(r1.total_bytes == Some(1_000_000));
        let progress = r1.downloaded_bytes as f64 / r1.total_bytes.unwrap() as f64;
        assert!(
            (progress - 0.1667).abs() < 0.01,
            "progress was {}",
            progress
        );
    }

    #[test]
    fn test_section_duration_after_transition() {
        let mut tracker = ProgressTracker::new("test_sect_trans".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let l1 = "__COMINE_PROGRESS__downloaded:1000,total:1000,speed:100,eta:0,filename:section1.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__downloaded:500,total:2000,speed:100,eta:15,filename:section2.mp4__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();

        let progress = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;
        assert!((progress - 0.5).abs() < 0.01, "progress was {}", progress);
    }

    #[test]
    fn test_section_duration_complete() {
        let mut tracker = ProgressTracker::new("test_sect_complete".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let l1 = "__COMINE_PROGRESS__downloaded:1000,total:1000,speed:100,eta:0,filename:s1.mp4__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__downloaded:2000,total:2000,speed:100,eta:0,filename:s2.mp4__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();

        let progress = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;
        assert!((progress - 1.0).abs() < 0.01, "progress was {}", progress);
    }

    #[test]
    fn test_section_duration_zero_total() {
        let mut tracker = ProgressTracker::new("test_zero_total".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let l1 = "__COMINE_PROGRESS__downloaded:500,total:NA,speed:100,eta:NA,filename:s1.mp4__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();

        let progress = r1.downloaded_bytes as f64 / r1.total_bytes.unwrap() as f64;
        assert!(
            progress < 0.01,
            "progress should be near 0, got {}",
            progress
        );
    }

    #[test]
    fn test_no_sections_normal_cumulative() {
        let mut tracker = ProgressTracker::new("test_no_sect".to_string());

        let l1 = "__COMINE_PROGRESS__downloaded:500,total:1000,speed:100,eta:5,filename:file.mp4__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();
        assert_eq!(r1.downloaded_bytes, 500);
        assert_eq!(r1.total_bytes, Some(1000));
    }

    #[test]
    fn test_realistic_youtube_download() {
        let mut tracker = ProgressTracker::new("yt_download".to_string());

        let video_lines = [
            (
                "downloaded:1000000,total:10000000,speed:2000000,eta:5,filename:My Video.f137.mp4",
                1_000_000u64,
            ),
            (
                "downloaded:5000000,total:10000000,speed:2500000,eta:2,filename:My Video.f137.mp4",
                5_000_000,
            ),
            (
                "downloaded:10000000,total:10000000,speed:3000000,eta:0,filename:My Video.f137.mp4",
                10_000_000,
            ),
        ];

        for (content, expected_dl) in &video_lines {
            let line = format!("__COMINE_PROGRESS__{}__COMINE_PROGRESS__", content);
            let r = tracker.parse_progress_line(&line).unwrap();
            assert_eq!(r.downloaded_bytes, *expected_dl);
            assert_eq!(r.total_bytes, Some(10_000_000));
        }

        let audio_lines = [
            (
                "downloaded:500000,total:2000000,speed:1000000,eta:2,filename:My Video.f251.webm",
                10_500_000u64,
            ),
            (
                "downloaded:2000000,total:2000000,speed:1500000,eta:0,filename:My Video.f251.webm",
                12_000_000,
            ),
        ];

        for (content, expected_dl) in &audio_lines {
            let line = format!("__COMINE_PROGRESS__{}__COMINE_PROGRESS__", content);
            let r = tracker.parse_progress_line(&line).unwrap();
            assert_eq!(r.downloaded_bytes, *expected_dl);
            assert_eq!(r.total_bytes, Some(12_000_000));
        }

        let files = tracker.captured_files();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"My Video.f137.mp4".to_string()));
        assert!(files.contains(&"My Video.f251.webm".to_string()));
    }

    #[test]
    fn test_ffmpeg_progress_basic() {
        let line = "frame=100 fps=25.0 q=28.0 size=1024kB time=00:00:04.00 bitrate=2097.2kbits/s speed=1.00x";
        let result = parse_ffmpeg_progress_line(line, "job1").unwrap();
        assert_eq!(result.downloaded_bytes, 1024 * 1000);
        assert!(result.speed.is_some());
    }

    #[test]
    fn test_ffmpeg_progress_no_time() {
        let line = "frame=0 fps=0.0 q=0.0 size=0kB";
        assert!(parse_ffmpeg_progress_line(line, "job1").is_none());
    }

    #[test]
    fn test_ffmpeg_progress_na_size() {
        let line = "frame=1 fps=1.0 q=28.0 size=N/A time=00:00:01.00 bitrate=N/A speed=1.00x";
        assert!(parse_ffmpeg_progress_line(line, "job1").is_none());
    }

    #[test]
    fn test_ffmpeg_progress_large_size() {
        let line = "frame=5000 fps=30.0 q=28.0 size=512000kB time=00:02:47.00 bitrate=25000.0kbits/s speed=2.0x";
        let result = parse_ffmpeg_progress_line(line, "job1").unwrap();
        assert_eq!(result.downloaded_bytes, 512_000 * 1000);
    }

    #[test]
    fn test_ffmpeg_progress_job_id() {
        let line =
            "frame=10 fps=10.0 q=28.0 size=100kB time=00:00:01.00 bitrate=800.0kbits/s speed=1.0x";
        let result = parse_ffmpeg_progress_line(line, "my-job-123").unwrap();
        assert_eq!(result.job_id, "my-job-123");
    }

    #[test]
    fn test_regression_progress_never_goes_backwards_on_phase_transition() {
        let mut tracker = ProgressTracker::new("regression1".to_string());

        let l1 = "__COMINE_PROGRESS__{downloaded:10000000,total:10000000,speed:5000000,eta:0,filename:video.f137.mp4}__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();
        assert_eq!(r1.downloaded_bytes, 10_000_000);
        assert_eq!(r1.total_bytes, Some(10_000_000));
        let pct1 = r1.downloaded_bytes as f64 / r1.total_bytes.unwrap() as f64;

        let l2 = "__COMINE_PROGRESS__{downloaded:0,total:2000000,speed:NA,eta:NA,filename:audio.f140.m4a}__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        let pct2 = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;

        assert!(
            pct2 >= pct1 - 0.001,
            "Progress went backwards! Before: {:.1}%, after: {:.1}%",
            pct1 * 100.0,
            pct2 * 100.0
        );

        let l3 = "__COMINE_PROGRESS__{downloaded:500000,total:2000000,speed:1000000,eta:2,filename:audio.f140.m4a}__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(l3).unwrap();
        assert_eq!(r3.downloaded_bytes, 10_500_000);
        assert_eq!(r3.total_bytes, Some(12_000_000));
    }

    #[test]
    fn test_regression_filename_with_commas() {
        let line = "__COMINE_PROGRESS__{downloaded:1024,total:2048,speed:512,eta:2,filename:My Video, Part 1.f137.mp4}__COMINE_PROGRESS__";
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.filename.as_deref(), Some("My Video, Part 1.f137.mp4"));
    }

    #[test]
    fn test_regression_filename_with_comma_no_spurious_transition() {
        let mut tracker = ProgressTracker::new("comma_test".to_string());

        let l1 = "__COMINE_PROGRESS__{downloaded:500,total:1000,speed:100,eta:5,filename:My Video, Part 1.f137.mp4}__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__{downloaded:800,total:1000,speed:100,eta:2,filename:My Video, Part 1.f137.mp4}__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        assert_eq!(r2.downloaded_bytes, 800);
        assert_eq!(r2.total_bytes, Some(1000));
    }

    #[test]
    fn test_regression_section_no_double_count_on_format_change() {
        let mut tracker = ProgressTracker::new("section_regression".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let l1 = "__COMINE_PROGRESS__{downloaded:1000,total:1000,speed:500,eta:0,filename:video_0.00-10.00.f137.mp4}__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__{downloaded:200,total:200,speed:500,eta:0,filename:video_0.00-10.00.f140.m4a}__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        let progress = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;
        assert!(
            progress < 0.5,
            "Progress should be ~33% (section 1 complete), got {:.1}%",
            progress * 100.0
        );

        let l3 = "__COMINE_PROGRESS__{downloaded:500,total:2000,speed:500,eta:3,filename:video_10.00-30.00.f137.mp4}__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(l3).unwrap();
        let progress3 = r3.downloaded_bytes as f64 / r3.total_bytes.unwrap() as f64;
        assert!(
            (progress3 - 0.5).abs() < 0.05,
            "Progress should be ~50%, got {:.1}%",
            progress3 * 100.0
        );
    }

    #[test]
    fn test_real_template_format_with_braces() {
        let line = r#"__COMINE_PROGRESS__{downloaded:5242880,total:10485760,total_estimate:NA,speed:2621440,eta:2,filename:/tmp/My Video.f137.mp4}__COMINE_PROGRESS__"#;
        let raw = extract_raw_progress(line).unwrap();
        assert_eq!(raw.downloaded, 5_242_880);
        assert_eq!(raw.total, Some(10_485_760));
        assert_eq!(raw.speed, Some(2_621_440));
        assert_eq!(raw.eta, Some(2));
        assert_eq!(raw.filename.as_deref(), Some("/tmp/My Video.f137.mp4"));
    }

    #[test]
    fn test_regression_section_progress_never_exceeds_100_percent() {
        let mut tracker = ProgressTracker::new("overflow_test".to_string());
        tracker.set_section_durations(vec![13.0, 14.0, 17.0, 27.0]);

        let l1 = "__COMINE_PROGRESS__{downloaded:4411363,total:4411363,speed:800000,eta:0,filename:video_0.0-13.0.webm}__COMINE_PROGRESS__";
        let r1 = tracker.parse_progress_line(l1).unwrap();
        let pct1 = r1.downloaded_bytes as f64 / r1.total_bytes.unwrap() as f64;
        assert!(
            pct1 <= 1.0,
            "Section 1 progress {:.1}% exceeds 100%",
            pct1 * 100.0
        );

        let l2 = "__COMINE_PROGRESS__{downloaded:7596717,total:7596717,speed:850000,eta:0,filename:video_33.0-47.0.webm}__COMINE_PROGRESS__";
        let r2 = tracker.parse_progress_line(l2).unwrap();
        let pct2 = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;
        assert!(
            pct2 <= 1.0,
            "Section 2 progress {:.1}% exceeds 100%",
            pct2 * 100.0
        );
        assert!(
            pct2 >= pct1,
            "Progress went backwards: {:.1}% → {:.1}%",
            pct1 * 100.0,
            pct2 * 100.0
        );

        let l3 = "__COMINE_PROGRESS__{downloaded:15258502,total:15258502,speed:900000,eta:0,filename:video_164.0-191.0.webm}__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(l3).unwrap();
        let pct3 = r3.downloaded_bytes as f64 / r3.total_bytes.unwrap() as f64;
        assert!(
            pct3 <= 1.0,
            "Section 4 progress {:.1}% exceeds 100%",
            pct3 * 100.0
        );
        assert!(
            pct3 >= pct2,
            "Progress went backwards: {:.1}% → {:.1}%",
            pct2 * 100.0,
            pct3 * 100.0
        );
    }

    #[test]
    fn test_regression_skipped_section_correct_index() {
        let mut tracker = ProgressTracker::new("skip_test".to_string());
        tracker.set_section_info(vec![13.0, 14.0, 17.0, 27.0], vec![0.0, 33.0, 114.0, 164.0]);

        let l1 = "__COMINE_PROGRESS__{downloaded:4000,total:4000,speed:500,eta:0,filename:video_0.0-13.0.webm}__COMINE_PROGRESS__";
        tracker.parse_progress_line(l1);

        let l2 = "__COMINE_PROGRESS__{downloaded:7000,total:7000,speed:500,eta:0,filename:video_33.0-47.0.webm}__COMINE_PROGRESS__";
        tracker.parse_progress_line(l2);

        let l3 = "__COMINE_PROGRESS__{downloaded:5000,total:15000,speed:500,eta:20,filename:video_164.0-191.0.webm}__COMINE_PROGRESS__";
        let r3 = tracker.parse_progress_line(l3).unwrap();

        let pct = r3.downloaded_bytes as f64 / r3.total_bytes.unwrap() as f64;
        assert!(
            (pct - 0.746).abs() < 0.05,
            "Expected ~74.6% with correct section skip, got {:.1}%",
            pct * 100.0
        );
    }

    #[test]
    fn test_feed_external_progress_with_sections() {
        let mut tracker = ProgressTracker::new("aria2_test".to_string());
        tracker.set_section_durations(vec![10.0, 20.0]);

        let r1 = tracker
            .feed_external_progress(500, Some(1000), Some(100), Some(5))
            .unwrap();
        assert_eq!(r1.total_bytes, Some(1_000_000));
        let pct1 = r1.downloaded_bytes as f64 / r1.total_bytes.unwrap() as f64;
        assert!(
            (pct1 - 0.167).abs() < 0.02,
            "Expected ~16.7%, got {:.1}%",
            pct1 * 100.0
        );
        assert_eq!(r1.speed, Some(100));
        assert_eq!(r1.eta, Some(5));

        let r2 = tracker
            .feed_external_progress(1000, Some(1000), Some(200), Some(0))
            .unwrap();
        let pct2 = r2.downloaded_bytes as f64 / r2.total_bytes.unwrap() as f64;
        assert!(
            (pct2 - 0.333).abs() < 0.02,
            "Expected ~33.3%, got {:.1}%",
            pct2 * 100.0
        );
        assert!(pct2 >= pct1, "Progress went backwards");
    }

    #[test]
    fn test_feed_external_progress_no_sections() {
        let mut tracker = ProgressTracker::new("aria2_nosect".to_string());

        let r1 = tracker
            .feed_external_progress(500, Some(1000), Some(100), Some(5))
            .unwrap();
        assert_eq!(r1.downloaded_bytes, 500);
        assert_eq!(r1.total_bytes, Some(1000));
        assert_eq!(r1.speed, Some(100));

        let r2 = tracker
            .feed_external_progress(800, Some(1000), Some(200), Some(2))
            .unwrap();
        assert_eq!(r2.downloaded_bytes, 800);
        assert_eq!(r2.total_bytes, Some(1000));
    }
}
