#[cfg(unix)]
pub async fn make_executable(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    tracing::info!("Setting executable permissions on: {:?}", path);

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;

    let mut perms = metadata.permissions();
    perms.set_mode(0o755);

    tokio::fs::set_permissions(path, perms)
        .await
        .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))?;

    tracing::info!("Successfully set executable permissions on {:?}", path);
    Ok(())
}
