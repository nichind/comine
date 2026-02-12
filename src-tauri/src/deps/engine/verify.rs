use std::path::{Path, PathBuf};

pub struct CmdOutputAsync {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_capture_async(exe: &Path, args: &[&str]) -> Result<CmdOutputAsync, String> {
    let mut cmd = crate::utils::new_command(exe);
    cmd.args(args);

    let output = cmd.output().await.map_err(|e| e.to_string())?;

    Ok(CmdOutputAsync {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

pub fn find_in_system_path(binary_name: &str) -> Option<PathBuf> {
    which::which(binary_name).ok()
}
