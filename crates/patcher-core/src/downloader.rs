use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use futures::stream::{StreamExt, TryStreamExt};
use gruf::thor::ThorArchive;
use gruf::GrufError;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::config::PatchServerInfo;
use crate::plist::{parse_patch_list, PatchItem};

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
            }
        }
    }

    bail!("All patch servers are unreachable.");
}

async fn probe_server(
    client: &reqwest::Client,
    server: &PatchServerInfo,
) -> Result<(Vec<PatchItem>, Url)> {
    let plist_url = Url::parse(&server.plist_url)
        .with_context(|| format!("Invalid plist_url: {}", server.plist_url))?;
    let patch_url = Url::parse(&server.patch_url)
        .with_context(|| format!("Invalid patch_url: {}", server.patch_url))?;

    let resp = client
        .get(plist_url.clone())
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .with_context(|| format!("Failed to fetch plist from {}", plist_url))?;

    if !resp.status().is_success() {
        bail!("Failed to get plist from {}: HTTP {}", plist_url, resp.status());
    }

    let text = resp.text().await?;
    let patch_list = parse_patch_list(&text);

    // If there is at least one patch, verify that patch_url can reach it
    if let Some(first_patch) = patch_list.first() {
        let first_patch_url = patch_url.join(&first_patch.file_name)?;
        let head_resp = client
            .head(first_patch_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        if let Ok(head) = head_resp {
            if !head.status().is_success() {
                bail!("Patch files not found on {}", patch_url);
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
    let last_speed_check = Arc::new(tokio::sync::Mutex::new(Instant::now()));

    // Spawn a background timer task to report download speed every 500ms
    let speed_completed = completed_count.clone();
    let speed_bytes = downloaded_bytes_recent.clone();
    let speed_bps_store = current_speed_bps.clone();
    let speed_active_file = active_file_name.clone();
    let speed_time = last_speed_check.clone();
    let speed_callback = progress_callback.clone();
    let speed_cancel = cancel_signal.clone();

    let speed_task = tokio::spawn(async move {
        while !speed_cancel.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut time_lock = speed_time.lock().await;
            let elapsed = time_lock.elapsed();
            if elapsed >= Duration::from_millis(500) {
                let bytes = speed_bytes.swap(0, Ordering::Relaxed);
                let speed = if elapsed.as_secs_f64() > 0.0 {
                    (bytes as f64 / elapsed.as_secs_f64()).round() as u64
                } else {
                    0
                };
                speed_bps_store.store(speed, Ordering::Relaxed);
                *time_lock = Instant::now();

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

            let file_url = base_url.join(&item.file_name)
                .with_context(|| format!("Invalid URL for patch file: {}", item.file_name))?;
            let local_path = dest_dir.join(&item.file_name);

            // Stream download to temporary or target file
            let mut file = File::create(&local_path).await
                .with_context(|| format!("Failed to create local patch file: {:?}", local_path))?;

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

                file.write_all(&chunk).await
                    .with_context(|| format!("Failed writing chunk for {}", item.file_name))?;

                bytes_tracker.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            }

            file.flush().await?;
            file.sync_all().await?;

            // Verify integrity if requested
            if ensure_integrity {
                validate_thor_archive(&local_path)
                    .with_context(|| format!("Integrity check failed for {}", item.file_name))?;
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
