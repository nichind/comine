use sha2::{Digest, Sha256};
use std::path::Path;

use crate::deps::error::{DepsError, DepsResult};

fn is_hex64(token: &str) -> bool {
    if token.len() != 64 {
        return false;
    }
    token.as_bytes().iter().all(|b: &u8| b.is_ascii_hexdigit())
}

pub fn extract_first_sha256(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let token = token.trim();
        if is_hex64(token) {
            return Some(token.to_ascii_lowercase());
        }
    }
    None
}

pub fn find_sha256_for_filename(sums_text: &str, filename: &str) -> Option<String> {
    for line in sums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.ends_with(filename)
            || trimmed.ends_with(&format!("/{}", filename))
            || trimmed.ends_with(&format!("*{}", filename))
        {
            if let Some(hash) = extract_first_sha256(trimmed) {
                return Some(hash);
            }
        }
    }

    extract_first_sha256(sums_text)
}

pub async fn try_fetch_sha256(
    urls: &[String],
    proxy_config: &crate::proxy::ProxyConfig,
    filename_hint: Option<&str>,
) -> Option<String> {
    for url in urls {
        match super::download::fetch_text(url, proxy_config).await {
            Ok(text) => {
                let parsed = match filename_hint {
                    Some(name) => find_sha256_for_filename(&text, name),
                    None => extract_first_sha256(&text),
                };

                if let Some(hash) = parsed {
                    return Some(hash);
                }
                tracing::warn!("Checksum fetched but could not be parsed: {}", url);
            }
            Err(e) => {
                tracing::warn!("Checksum fetch failed from {}: {}", url, e);
            }
        }
    }
    None
}

pub async fn verify_sha256(path: &Path, expected: &str, dep_name: &'static str) -> DepsResult<()> {
    let path = path.to_path_buf();
    let expected = expected.to_lowercase();

    let actual = tokio::task::spawn_blocking(move || -> Result<String, std::io::Error> {
        use std::io::Read;

        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(format!("{:x}", hash))
    })
    .await
    .map_err(|e| DepsError::other(format!("Hash task failed: {}", e)))?
    .map_err(DepsError::Io)?;

    if actual != expected {
        return Err(DepsError::checksum_mismatch(dep_name, expected, actual));
    }

    tracing::info!("Checksum verified for {}: {}", dep_name, actual);
    Ok(())
}
