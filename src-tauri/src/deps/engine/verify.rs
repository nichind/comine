use std::path::Path;

#[cfg(target_os = "windows")]
use crate::utils::CommandHideConsole;

pub struct CmdOutputAsync {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_capture_async(exe: &Path, args: &[&str]) -> Result<CmdOutputAsync, String> {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(args);

    #[cfg(target_os = "windows")]
    cmd.hide_console();

    let output = cmd.output().await.map_err(|e| e.to_string())?;

    Ok(CmdOutputAsync {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}
