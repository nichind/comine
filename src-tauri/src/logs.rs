use log::info;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn get_logs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let logs_dir = app_data_dir.join("logs");

    if !logs_dir.exists() {
        std::fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs dir: {}", e))?;
    }

    Ok(logs_dir)
}

#[tauri::command]
pub async fn get_log_file_path(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        return Err("Log files not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let logs_dir = get_logs_dir(&app)?;
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let log_file = logs_dir.join(format!("comine_{}.log", timestamp));
        Ok(log_file.to_string_lossy().to_string())
    }
}

#[tauri::command]
pub async fn append_log(
    _app: AppHandle,
    session_file: String,
    entry: String,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let path = std::path::Path::new(&session_file);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to open log file: {}", e))?;

        writeln!(file, "{}", entry).map_err(|e| format!("Failed to write to log file: {}", e))?;
        Ok(())
    }
}

#[tauri::command]
pub async fn cleanup_old_logs(app: AppHandle, keep_sessions: usize) -> Result<usize, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(0);
    }

    #[cfg(not(target_os = "android"))]
    {
        let logs_dir = get_logs_dir(&app)?;

        let mut log_files: Vec<_> = std::fs::read_dir(&logs_dir)
            .map_err(|e| format!("Failed to read logs dir: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "log")
                    .unwrap_or(false)
            })
            .collect();

        log_files.sort_by(|a, b| {
            let time_a = a.metadata().and_then(|m| m.modified()).ok();
            let time_b = b.metadata().and_then(|m| m.modified()).ok();
            time_b.cmp(&time_a)
        });

        let mut deleted = 0;
        for entry in log_files.into_iter().skip(keep_sessions) {
            if std::fs::remove_file(entry.path()).is_ok() {
                deleted += 1;
                info!("Deleted old log file: {:?}", entry.path());
            }
        }

        Ok(deleted)
    }
}

#[tauri::command]
pub async fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return Err("Not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let logs_dir = get_logs_dir(&app)?;

        #[cfg(target_os = "windows")]
        {
            use crate::utils::StdCommandHideConsole;
            std::process::Command::new("explorer")
                .arg(&logs_dir)
                .hide_console()
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&logs_dir)
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }

        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&logs_dir)
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }

        Ok(())
    }
}

#[tauri::command]
pub async fn get_logs_folder_path(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        return Err("Not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let logs_dir = get_logs_dir(&app)?;
        Ok(logs_dir.to_string_lossy().to_string())
    }
}

#[tauri::command]
pub async fn read_session_logs(
    session_file: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(vec![]);
    }

    #[cfg(not(target_os = "android"))]
    {
        let path = std::path::Path::new(&session_file);
        if !path.exists() {
            return Ok(vec![]);
        }

        let file =
            std::fs::File::open(path).map_err(|e| format!("Failed to open log file: {}", e))?;
        let reader = BufReader::new(file);
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let lines: Vec<String> = reader
            .lines()
            .skip(offset)
            .take(limit)
            .filter_map(|line| line.ok())
            .collect();

        Ok(lines)
    }
}

#[tauri::command]
pub async fn get_session_log_count(session_file: String) -> Result<usize, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(0);
    }

    #[cfg(not(target_os = "android"))]
    {
        let path = std::path::Path::new(&session_file);
        if !path.exists() {
            return Ok(0);
        }

        let file =
            std::fs::File::open(path).map_err(|e| format!("Failed to open log file: {}", e))?;
        let reader = BufReader::new(file);
        Ok(reader.lines().count())
    }
}
