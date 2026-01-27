#[cfg(not(target_os = "android"))]
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::error!("Mutex was poisoned by a panicked thread, recovering");
            poisoned.into_inner()
        }
    }
}

#[derive(serde::Serialize)]
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

    // Find the disk that contains this path
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

#[cfg(target_os = "windows")]
pub trait CommandHideConsole {
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
pub trait StdCommandHideConsole {
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
