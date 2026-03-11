use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tracing::{error, info, warn};

use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::{find_in_system_path, run_capture_async};
use crate::types::{DependencyStatus, InstallProgress};

const EVENT_PROGRESS: &str = "edge-tts-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "edge-tts.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "edge-tts";

fn get_venv_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("podcast-venv"))
        .map_err(|e| format!("Failed to get app data dir: {}", e))
}

fn get_venv_bin(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let venv = get_venv_dir(app)?;
    #[cfg(target_os = "windows")]
    {
        Ok(venv.join("Scripts").join(format!("{}.exe", name)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(venv.join("bin").join(name))
    }
}

/// Resolve edge-tts binary path: check venv first, then system PATH.
pub fn resolve_edge_tts_path(app: &AppHandle) -> Option<PathBuf> {
    // Check venv-local binary first
    if let Ok(venv_bin) = get_venv_bin(app, "edge-tts") {
        if venv_bin.exists() {
            return Some(venv_bin);
        }
    }
    // Fall back to system PATH
    find_in_system_path(BINARY_NAME)
}

/// Find a usable Python 3 executable on the system.
async fn find_python3() -> Option<PathBuf> {
    // Try `python3` first (preferred on Unix/macOS), then `python` (Windows fallback)
    for candidate in &["python3", "python"] {
        if let Some(path) = find_in_system_path(candidate) {
            // Verify it's actually Python 3 by checking --version output
            if let Ok(out) = run_capture_async(&path, &["--version"]).await {
                let combined = format!("{} {}", out.stdout, out.stderr);
                if combined.contains("Python 3") {
                    return Some(path);
                }
            }
        }
    }
    None
}

pub async fn check_edge_tts(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    #[cfg(mobile)]
    {
        let _ = check_updates;
        return Ok(DependencyStatus::not_installed());
    }

    #[cfg(desktop)]
    {
        let _ = check_updates; // edge-tts has no structured GitHub releases to compare against

        let edge_tts_path = match resolve_edge_tts_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        // Try `edge-tts --version`; fall back to `pip show edge-tts` for the version string.
        match run_capture_async(&edge_tts_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                let version = output.stdout.trim().to_string();
                info!("edge-tts version (--version): {}", version);
                Ok(DependencyStatus::installed(
                    version,
                    edge_tts_path.to_string_lossy().to_string(),
                ))
            }
            Ok(_) | Err(_) => {
                // edge-tts binary found but --version failed or returned non-zero.
                // Try pip show to get the installed package version.
                let version = get_pip_version(&app).await.unwrap_or_default();
                if version.is_empty() {
                    warn!("edge-tts binary found at {:?} but version query failed", edge_tts_path);
                    Ok(DependencyStatus::not_installed())
                } else {
                    info!("edge-tts version (pip show): {}", version);
                    Ok(DependencyStatus::installed(
                        version,
                        edge_tts_path.to_string_lossy().to_string(),
                    ))
                }
            }
        }
    }
}

/// Query the installed version via `pip show edge-tts`, checking the venv pip first.
async fn get_pip_version(app: &AppHandle) -> Option<String> {
    // Prefer venv pip
    let pip_path = get_venv_bin(app, "pip").ok().filter(|p| p.exists()).or_else(|| {
        find_in_system_path("pip3").or_else(|| find_in_system_path("pip"))
    })?;

    let out = run_capture_async(&pip_path, &["show", "edge-tts"]).await.ok()?;
    for line in out.stdout.lines() {
        if let Some(rest) = line.strip_prefix("Version:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

pub async fn install_edge_tts(app: AppHandle) -> Result<String, String> {
    #[cfg(mobile)]
    {
        return Err("edge-tts installation on mobile is not supported.".to_string());
    }

    #[cfg(desktop)]
    {
        let progress = ProgressEmitter::new(&app, EVENT_PROGRESS);

        progress.emit(InstallProgress {
            stage: "checking".to_string(),
            progress: 5,
            message: "Checking for Python 3...".to_string(),
            ..Default::default()
        });

        // Ensure Python 3 is available
        let python_path = find_python3().await.ok_or_else(|| {
            "Python 3 is not installed or not in PATH. \
             Please install Python 3 and try again."
                .to_string()
        })?;

        info!("Using Python 3 at: {:?}", python_path);

        let venv_dir = get_venv_dir(&app)?;

        // Create venv if it doesn't already exist
        if !venv_dir.exists() {
            progress.emit(InstallProgress {
                stage: "creating venv".to_string(),
                progress: 20,
                message: "Creating podcast virtual environment...".to_string(),
                ..Default::default()
            });

            info!("Creating podcast-venv at {:?}", venv_dir);
            let out = crate::utils::new_command(&python_path)
                .args(["-m", "venv", &venv_dir.to_string_lossy()])
                .output()
                .await
                .map_err(|e| format!("Failed to spawn python -m venv: {}", e))?;

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                error!("Failed to create venv: {}", stderr);
                return Err(format!("Failed to create podcast-venv: {}", stderr));
            }

            info!("podcast-venv created successfully");
        } else {
            info!("podcast-venv already exists at {:?}, skipping creation", venv_dir);
        }

        // Resolve venv pip
        let venv_pip = get_venv_bin(&app, "pip")?;
        if !venv_pip.exists() {
            return Err(format!(
                "Expected pip in venv but not found at {:?}. \
                 The venv may be broken — try deleting it and reinstalling.",
                venv_pip
            ));
        }

        progress.emit(InstallProgress {
            stage: "installing".to_string(),
            progress: 40,
            message: "Installing edge-tts via pip...".to_string(),
            ..Default::default()
        });

        info!("Installing edge-tts into {:?}", venv_dir);
        let install_out = crate::utils::new_command(&venv_pip)
            .args(["install", "--upgrade", "edge-tts"])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn pip install: {}", e))?;

        if !install_out.status.success() {
            let stderr = String::from_utf8_lossy(&install_out.stderr).trim().to_string();
            error!("pip install edge-tts failed: {}", stderr);
            return Err(format!("Failed to install edge-tts: {}", stderr));
        }

        info!("edge-tts pip install succeeded");

        progress.emit(InstallProgress {
            stage: "verifying".to_string(),
            progress: 80,
            message: "Verifying edge-tts installation...".to_string(),
            ..Default::default()
        });

        // Verify the binary is now resolvable and get version
        let edge_tts_path = resolve_edge_tts_path(&app)
            .ok_or_else(|| "edge-tts was installed but the binary was not found.".to_string())?;

        let version = get_pip_version(&app).await.unwrap_or_else(|| "unknown".to_string());

        progress.emit(InstallProgress {
            stage: "done".to_string(),
            progress: 100,
            message: format!("edge-tts {} installed", version),
            ..Default::default()
        });

        info!("edge-tts installed at {:?}, version: {}", edge_tts_path, version);
        Ok(version)
    }
}

pub async fn uninstall_edge_tts(app: AppHandle) -> Result<(), String> {
    #[cfg(mobile)]
    {
        return Err("edge-tts is not supported on mobile.".to_string());
    }

    #[cfg(desktop)]
    {
        // Prefer uninstalling from the venv pip to avoid touching system packages
        let venv_pip = get_venv_bin(&app, "pip").ok().filter(|p| p.exists());

        if let Some(pip) = venv_pip {
            info!("Uninstalling edge-tts from podcast-venv via {:?}", pip);
            let out = crate::utils::new_command(&pip)
                .args(["uninstall", "-y", "edge-tts"])
                .output()
                .await
                .map_err(|e| format!("Failed to spawn pip uninstall: {}", e))?;

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                warn!("pip uninstall edge-tts returned error: {}", stderr);
                // Non-fatal: the package may simply not have been installed in this venv
            }
            return Ok(());
        }

        // Fall back to system pip3 / pip
        let system_pip = find_in_system_path("pip3")
            .or_else(|| find_in_system_path("pip"))
            .ok_or_else(|| {
                "Could not find pip to uninstall edge-tts. \
                 Please run `pip uninstall -y edge-tts` manually."
                    .to_string()
            })?;

        info!("Uninstalling edge-tts via system pip at {:?}", system_pip);
        let out = crate::utils::new_command(&system_pip)
            .args(["uninstall", "-y", "edge-tts"])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn pip uninstall: {}", e))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(format!("Failed to uninstall edge-tts: {}", stderr));
        }

        Ok(())
    }
}
