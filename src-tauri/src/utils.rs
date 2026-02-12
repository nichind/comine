#[cfg(not(target_os = "android"))]
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!("Mutex was poisoned by a panicked thread, recovering");
            poisoned.into_inner()
        }
    }
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskSpaceInfo {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub available_gb: f64,
    pub total_gb: f64,
    pub used_percent: f64,
}

#[cfg(not(target_os = "android"))]
pub fn get_disk_space_for_path(path: &str) -> Option<DiskSpaceInfo> {
    use sysinfo::Disks;

    let path = Path::new(path);
    let disks = Disks::new_with_refreshed_list();

    let mut best_match: Option<&sysinfo::Disk> = None;
    let mut best_mount_len = 0;

    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let mount_len = mount.as_os_str().len();
            if mount_len > best_mount_len {
                best_mount_len = mount_len;
                best_match = Some(disk);
            }
        }
    }

    best_match.map(|disk| {
        let available = disk.available_space();
        let total = disk.total_space();
        let used = total.saturating_sub(available);
        DiskSpaceInfo {
            available_bytes: available,
            total_bytes: total,
            available_gb: available as f64 / 1_073_741_824.0,
            total_gb: total as f64 / 1_073_741_824.0,
            used_percent: if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        }
    })
}

#[cfg(target_os = "android")]
pub fn get_disk_space_for_path(path: &str) -> Option<DiskSpaceInfo> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let c_path = CString::new(path).ok()?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };

    if result != 0 {
        tracing::warn!("statvfs failed for path: {}", path);
        return None;
    }

    let stat = unsafe { stat.assume_init() };

    let block_size = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block_size;
    let available = stat.f_bavail as u64 * block_size; // Available to non-root users
    let used = total.saturating_sub(stat.f_bfree as u64 * block_size);

    Some(DiskSpaceInfo {
        available_bytes: available,
        total_bytes: total,
        available_gb: available as f64 / 1_073_741_824.0,
        total_gb: total as f64 / 1_073_741_824.0,
        used_percent: if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        },
    })
}

#[cfg(target_os = "windows")]
trait CommandHideConsole {
    fn hide_console(&mut self) -> &mut Self;
}

#[cfg(target_os = "windows")]
impl CommandHideConsole for tokio::process::Command {
    fn hide_console(&mut self) -> &mut Self {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}

#[cfg(target_os = "windows")]
trait StdCommandHideConsole {
    fn hide_console(&mut self) -> &mut Self;
}

#[cfg(target_os = "windows")]
impl StdCommandHideConsole for std::process::Command {
    fn hide_console(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}

pub fn new_command<P: AsRef<std::ffi::OsStr>>(program: P) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.hide_console();
    }
    cmd
}

pub fn new_std_command<P: AsRef<std::ffi::OsStr>>(program: P) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.hide_console();
    }
    cmd
}

use crate::orchestrator::types::constants::USER_AGENT;

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

pub fn http_client_with_proxy(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10));

    match proxy_url {
        Some(url) if !url.is_empty() => {
            let proxy =
                reqwest::Proxy::all(url).map_err(|e| format!("Invalid proxy URL: {}", e))?;
            builder = builder.proxy(proxy);
        }
        _ => {
            builder = builder.no_proxy();
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

#[cfg(not(target_os = "android"))]
pub async fn get_media_duration(ffprobe_path: &Path, file_path: &str) -> Option<f64> {
    let mut cmd = new_command(ffprobe_path);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        file_path,
    ]);

    let output = cmd.output().await.ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}
