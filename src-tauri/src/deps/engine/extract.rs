use std::io::Read;
use std::path::Path;

use tracing::{error, info, warn};

use crate::deps::error::{DepsResult, ExtractError};

pub type FileMatcher = fn(&str) -> bool;

pub struct ZipExtractConfig {
    pub matcher: FileMatcher,
    pub dest_name: &'static str,
    pub extract_all: bool,
}

pub async fn extract_from_zip(
    archive_path: &Path,
    bin_dir: &Path,
    config: ZipExtractConfig,
) -> DepsResult<()> {
    if !archive_path.exists() {
        error!("Archive file does not exist: {:?}", archive_path);
        return Err(ExtractError::archive_not_found(archive_path.display().to_string()).into());
    }

    let metadata = tokio::fs::metadata(archive_path).await?;

    info!(
        "Archive size: {} bytes at {:?}",
        metadata.len(),
        archive_path
    );

    if metadata.len() == 0 {
        return Err(ExtractError::EmptyArchive.into());
    }

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| ExtractError::io(format!("Failed to open archive: {}", e)))?;

        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| ExtractError::InvalidFormat(e.to_string()))?;

        let mut found = false;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| ExtractError::io(format!("Failed to read zip entry: {}", e)))?;

            let name = file.name().to_string();

            if (config.matcher)(&name) && !file.is_dir() {
                let dest_path = bin_dir.join(config.dest_name);

                let mut contents = Vec::new();
                file.read_to_end(&mut contents).map_err(|e| {
                    ExtractError::io(format!("Failed to read file from zip: {}", e))
                })?;

                std::fs::write(&dest_path, &contents)
                    .map_err(|e| ExtractError::io(format!("Failed to write file: {}", e)))?;

                info!(
                    "Extracted {} from {} to {:?}",
                    config.dest_name, name, dest_path
                );
                found = true;

                if !config.extract_all {
                    break;
                }
            }
        }

        if !found {
            warn!(
                "No matching files found in archive for {}",
                config.dest_name
            );
        }

        Ok::<(), ExtractError>(())
    })
    .await
    .map_err(|e| ExtractError::io(format!("Task failed: {}", e)))??;

    Ok(())
}

pub async fn extract_from_zip_multiple(
    archive_path: &Path,
    bin_dir: &Path,
    matchers: &[(FileMatcher, &'static str)],
) -> DepsResult<()> {
    if !archive_path.exists() {
        error!("Archive file does not exist: {:?}", archive_path);
        return Err(ExtractError::archive_not_found(archive_path.display().to_string()).into());
    }

    let metadata = tokio::fs::metadata(archive_path).await?;

    info!(
        "Archive size: {} bytes at {:?}",
        metadata.len(),
        archive_path
    );

    if metadata.len() == 0 {
        return Err(ExtractError::EmptyArchive.into());
    }

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();
    let matchers: Vec<_> = matchers.to_vec();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| ExtractError::io(format!("Failed to open archive: {}", e)))?;

        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| ExtractError::InvalidFormat(e.to_string()))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| ExtractError::io(format!("Failed to read zip entry: {}", e)))?;

            let name = file.name().to_string();

            for (matcher, dest_name) in &matchers {
                if matcher(&name) && !file.is_dir() {
                    let dest_path = bin_dir.join(dest_name);

                    let mut contents = Vec::new();
                    file.read_to_end(&mut contents).map_err(|e| {
                        ExtractError::io(format!("Failed to read file from zip: {}", e))
                    })?;

                    std::fs::write(&dest_path, &contents)
                        .map_err(|e| ExtractError::io(format!("Failed to write file: {}", e)))?;

                    info!("Extracted {} to {:?}", dest_name, dest_path);
                    break;
                }
            }
        }

        Ok::<(), ExtractError>(())
    })
    .await
    .map_err(|e| ExtractError::io(format!("Task failed: {}", e)))??;

    Ok(())
}
