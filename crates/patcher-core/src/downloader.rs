use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use futures::stream::{StreamExt, TryStreamExt};
use gruf::thor::ThorArchive;
use gruf::GrufError;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use url::Url;

use crate::config::PatchServerInfo;
use crate::patching::is_windows_reserved_device_name;
use crate::plist::{parse_patch_list, PatchItem};

const MAX_PLIST_BYTES: usize = 10 * 1024 * 1024;

/// Validates that `raw` is a syntactically valid `http://` or `https://` URL with a non-empty host.
pub fn validate_http_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("URL cannot be empty");
    }
    let lower = trimmed.to_ascii_lowercase();
    let after_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    match after_scheme {
        Some(rest) if !rest.is_empty() && !rest.starts_with('/') => {}
        _ => bail!("URL must start with http:// or https:// and include a valid host: {}", raw),
    }

    let parsed = Url::parse(trimmed).with_context(|| format!("Invalid URL: {}", raw))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("Unsupported URL scheme {:?} (only http:// and https:// are allowed)", parsed.scheme());
    }
    if parsed.host_str().map_or(true, |h| h.is_empty()) {
        bail!("URL is missing a valid host: {}", raw);
    }
    Ok(parsed)
}

/// Validates a relative patch filename from `plist.txt` against path traversal,
/// scheme/drive injection, backslashes, and Windows reserved device names.
pub fn validate_patch_file_name(name: &str) -> Result<&str> {
    if name.trim().is_empty() {
        bail!("Patch file name cannot be empty");
    }
    if name.chars().any(|c| c.is_control()) {
        bail!("Patch file name contains control characters: {:?}", name);
    }
    if name.contains('\\') || name.contains(':') || name.starts_with('/') || name.contains("//") {
        bail!("Disallowed characters or path prefix in patch file name: {:?}", name);
    }

    for seg in name.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            bail!("Invalid path segment {:?} in patch file name {:?}", seg, name);
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            bail!("Trailing dot or space not allowed in patch file name {:?}", name);
        }
        if is_windows_reserved_device_name(seg) {
            bail!("Windows reserved device name not allowed in patch file name {:?}", name);
        }
    }

    Ok(name)
}

/// Resolves a validated patch file name against `base_url`, ensuring same-origin HTTP(S).
pub fn resolve_patch_download_url(base_url: &Url, file_name: &str) -> Result<Url> {
    let safe_name = validate_patch_file_name(file_name)?;
    let resolved = base_url
        .join(safe_name)
        .with_context(|| format!("Invalid URL for patch file: {}", file_name))?;
    if !matches!(resolved.scheme(), "http" | "https") {
        bail!("Resolved patch URL uses disallowed scheme: {}", resolved.scheme());
    }
    if resolved.origin() != base_url.origin() {
        bail!(
            "Patch file URL {:?} escapes configured server origin {:?}",
            resolved.as_str(),
            base_url.origin().ascii_serialization()
        );
    }
    Ok(resolved)
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded_files: usize,
    pub total_files: usize,
    pub bytes_per_sec: u64,
    pub current_file: String,
}

#[derive(Debug, Clone)]
pub struct DownloadedPatch {
    pub info: PatchItem,
    pub local_path: PathBuf,
}

pub async fn find_available_patch_server(
    client: &reqwest::Client,
    servers: &[PatchServerInfo],
    preferred: Option<&str>,
) -> Result<(Vec<PatchItem>, Url)> {
    if servers.is_empty() {
        bail!("No patch servers configured.");
    }

    let mut errors = Vec::new();

    // Try preferred server first if specified
    if let Some(pref_name) = preferred {
        if let Some(srv) = servers.iter().find(|s| s.name == pref_name) {
            match probe_server(client, srv).await {
                Ok(res) => {
                    log::info!("Connected to preferred patch server '{}'", srv.name);
                    return Ok(res);
                }
                Err(err) => {
                    log::warn!("Preferred server '{}' unavailable: {:#}", srv.name, err);
                    errors.push(format!("{}: {:#}", srv.name, err));
                }
            }
        }
    }

    // Probe other servers
    for srv in servers {
        if preferred == Some(&srv.name) {
            continue;
        }
        match probe_server(client, srv).await {
            Ok(res) => {
                log::info!("Connected to patch server '{}'", srv.name);
                return Ok(res);
            }
            Err(err) => {
                log::warn!("Server '{}' unavailable: {:#}", srv.name, err);
                errors.push(format!("{}: {:#}", srv.name, err));
            }
        }
    }

    if errors.is_empty() {
        bail!("All patch servers are unreachable.");
    } else {
        bail!("All patch servers are unreachable ({})", errors.join("; "));
    }
}

async fn probe_server(
    client: &reqwest::Client,
    server: &PatchServerInfo,
) -> Result<(Vec<PatchItem>, Url)> {
    let plist_url = validate_http_url(&server.plist_url)
        .with_context(|| format!("Invalid plist_url: {}", server.plist_url))?;
    let trimmed_patch = server.patch_url.trim();
    let normalized_patch_url = if trimmed_patch.ends_with('/') {
        trimmed_patch.to_string()
    } else {
        format!("{}/", trimmed_patch)
    };
    let patch_url = validate_http_url(&normalized_patch_url)
        .with_context(|| format!("Invalid patch_url: {}", server.patch_url))?;

    let mut resp = client
        .get(plist_url.clone())
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .with_context(|| format!("Failed to fetch plist from {}", plist_url))?;

    if !resp.status().is_success() {
        bail!("Failed to get plist from {}: HTTP {}", plist_url, resp.status());
    }

    if let Some(content_len) = resp.content_length() {
        if content_len > MAX_PLIST_BYTES as u64 {
            bail!(
                "Patch list from {} exceeds maximum size ({} bytes)",
                plist_url,
                MAX_PLIST_BYTES
            );
        }
    }

    let mut plist_bytes = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .with_context(|| format!("Failed reading plist from {}", plist_url))?
    {
        if plist_bytes.len().saturating_add(chunk.len()) > MAX_PLIST_BYTES {
            bail!(
                "Patch list from {} exceeds maximum allowed size ({} bytes)",
                plist_url,
                MAX_PLIST_BYTES
            );
        }
        plist_bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&plist_bytes);
    let patch_list = parse_patch_list(&text);

    // If there is at least one patch, verify that patch_url can reach it
    if let Some(first_patch) = patch_list.first() {
        let first_patch_url = resolve_patch_download_url(&patch_url, &first_patch.file_name)?;
        let head_resp = client
            .head(first_patch_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        if let Ok(head) = head_resp {
            let status = head.status();
            // Only fail if server explicitly returns 404 Not Found or 5xx; some servers block HEAD with 405/403
            if status == reqwest::StatusCode::NOT_FOUND || status.is_server_error() {
                bail!("Patch file '{}' not found on {} (HTTP {})", first_patch.file_name, patch_url, status);
            }
        }
    }

    Ok((patch_list, patch_url))
}

pub async fn download_patches_concurrent<F>(
    client: &reqwest::Client,
    base_url: &Url,
    patches: Vec<PatchItem>,
    destination_dir: &Path,
    ensure_integrity: bool,
    max_concurrency: usize,
    cancel_signal: Arc<AtomicBool>,
    progress_callback: F,
) -> Result<Vec<DownloadedPatch>>
where
    F: Fn(DownloadProgress) + Send + Sync + 'static,
{
    let total_files = patches.len();
    if total_files == 0 {
        return Ok(Vec::new());
    }

    let progress_callback = Arc::new(progress_callback);
    let completed_count = Arc::new(AtomicUsize::new(0));
    let downloaded_bytes_recent = Arc::new(AtomicU64::new(0));
    let current_speed_bps = Arc::new(AtomicU64::new(0));
    let active_file_name = Arc::new(std::sync::Mutex::new(String::new()));

    // Spawn a background timer task to report download speed every 500ms
    let speed_completed = completed_count.clone();
    let speed_bytes = downloaded_bytes_recent.clone();
    let speed_bps_store = current_speed_bps.clone();
    let speed_active_file = active_file_name.clone();
    let speed_callback = progress_callback.clone();
    let speed_cancel = cancel_signal.clone();

    let speed_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.tick().await;
        let mut last_tick = Instant::now();

        while !speed_cancel.load(Ordering::Relaxed) {
            interval.tick().await;
            let elapsed = last_tick.elapsed();
            let bytes = speed_bytes.swap(0, Ordering::Relaxed);
            let speed = if elapsed.as_secs_f64() > 0.0 {
                (bytes as f64 / elapsed.as_secs_f64()).round() as u64
            } else {
                0
            };
            speed_bps_store.store(speed, Ordering::Relaxed);
            last_tick = Instant::now();

            let current_file = speed_active_file
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();

            speed_callback(DownloadProgress {
                downloaded_files: speed_completed.load(Ordering::Relaxed),
                total_files,
                bytes_per_sec: speed,
                current_file,
            });
        }
    });

    let concurrency = if max_concurrency == 0 { 16 } else { max_concurrency.min(32) };

    let results = futures::stream::iter(patches.into_iter().map(|item| {
        let client = client.clone();
        let base_url = base_url.clone();
        let dest_dir = destination_dir.to_path_buf();
        let cancel = cancel_signal.clone();
        let completed = completed_count.clone();
        let bytes_tracker = downloaded_bytes_recent.clone();
        let speed_bps_ref = current_speed_bps.clone();
        let active_file_ref = active_file_name.clone();
        let cb = progress_callback.clone();

        async move {
            if cancel.load(Ordering::Relaxed) {
                bail!("Download cancelled.");
            }

            if let Ok(mut guard) = active_file_ref.lock() {
                *guard = item.file_name.clone();
            }

            let file_url = resolve_patch_download_url(&base_url, &item.file_name)?;
            let local_path = dest_dir.join(format!("patch_{:010}.thor", item.index));

            let download_res: Result<()> = async {
                let file = File::create(&local_path).await
                    .with_context(|| format!("Failed to create local patch file: {:?}", local_path))?;
                let mut writer = BufWriter::with_capacity(64 * 1024, file);

                let mut resp = client.get(file_url).send().await
                    .with_context(|| format!("Failed to request patch: {}", item.file_name))?;

                if !resp.status().is_success() {
                    bail!("Failed to download {}: HTTP {}", item.file_name, resp.status());
                }

                while let Some(chunk) = resp.chunk().await
                    .with_context(|| format!("Failed reading chunk for {}", item.file_name))?
                {
                    if cancel.load(Ordering::Relaxed) {
                        bail!("Download cancelled.");
                    }

                    writer.write_all(&chunk).await
                        .with_context(|| format!("Failed writing chunk for {}", item.file_name))?;

                    bytes_tracker.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }

                writer.flush().await?;
                drop(writer);

                // Verify integrity if requested
                if ensure_integrity {
                    validate_thor_archive(&local_path)
                        .with_context(|| format!("Integrity check failed for {}", item.file_name))?;
                }

                Ok(())
            }
            .await;

            if let Err(err) = download_res {
                let _ = tokio::fs::remove_file(&local_path).await;
                return Err(err);
            }

            let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
            if let Ok(mut guard) = active_file_ref.lock() {
                *guard = item.file_name.clone();
            }
            cb(DownloadProgress {
                downloaded_files: done,
                total_files,
                bytes_per_sec: speed_bps_ref.load(Ordering::Relaxed),
                current_file: item.file_name.clone(),
            });

            Ok(DownloadedPatch {
                info: item,
                local_path,
            })
        }
    }))
    .buffer_unordered(concurrency)
    .try_collect::<Vec<DownloadedPatch>>()
    .await;

    speed_task.abort();

    let mut list = results?;
    list.sort_by_key(|p| p.info.index);
    Ok(list)
}

fn validate_thor_archive(path: &Path) -> Result<()> {
    let mut archive = ThorArchive::open(path)
        .with_context(|| format!("Failed to open downloaded THOR archive at {:?}", path))?;

    match archive.is_valid() {
        Ok(true) => Ok(()),
        Ok(false) => bail!("THOR archive integrity file mismatch (corrupted download)"),
        Err(GrufError::EntryNotFound) => {
            // No integrity record in this thor file, treated as valid
            Ok(())
        }
        Err(e) => bail!("Failed to verify THOR integrity: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_http_url() {
        assert!(validate_http_url("https://patch.example.com/plist.txt").is_ok());
        assert!(validate_http_url("http://127.0.0.1:8080/patch/").is_ok());

        for bad in ["", "ftp://example.com/a", "file:///C:/plist.txt", "javascript:alert(1)", "http://"] {
            assert!(validate_http_url(bad).is_err(), "Expected {:?} to fail", bad);
        }
    }

    #[test]
    fn test_validate_patch_file_name_and_resolve_url() {
        let base = Url::parse("https://cdn.example.com/patches/").unwrap();
        assert_eq!(
            resolve_patch_download_url(&base, "001_welcome.thor").unwrap().as_str(),
            "https://cdn.example.com/patches/001_welcome.thor"
        );
        assert_eq!(
            resolve_patch_download_url(&base, "2026/002_event.thor").unwrap().as_str(),
            "https://cdn.example.com/patches/2026/002_event.thor"
        );

        for bad in [
            "",
            "../secret.thor",
            "2026/../secret.thor",
            "C:\\evil.thor",
            "sub\\001.thor",
            "/root.thor",
            "//evil.com/x.thor",
            "https://evil.com/x.thor",
            "NUL",
            "patches/CON.thor",
            "patch.thor.",
            "patch.thor ",
            "bad\n.thor",
        ] {
            assert!(
                resolve_patch_download_url(&base, bad).is_err(),
                "Expected {:?} to be rejected",
                bad
            );
        }
    }
}
