pub mod cache;
pub mod config;
pub mod downloader;
pub mod launcher;
pub mod lock;
pub mod overlay;
pub mod pe;
pub mod patching;
pub mod plist;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use config::PatcherConfig;
use downloader::{
    download_patches_concurrent, find_available_patch_server, DownloadProgress,
};
use lock::FileLock;
use patching::{apply_patch_queue, InstallProgress, PatchingResult};

#[derive(Debug, Clone)]
pub enum EngineStatus {
    Idle,
    CheckingForUpdates,
    Downloading(DownloadProgress),
    Installing(InstallProgress),
    UpToDate,
    SelfUpdated,
    Error(String),
}

pub struct PatcherEngine {
    pub config: PatcherConfig,
    pub game_dir: PathBuf,
    pub cache_path: PathBuf,
    pub lock_path: PathBuf,
    pub cancel_signal: Arc<AtomicBool>,
}

impl PatcherEngine {
    pub fn new(config: PatcherConfig, game_dir: impl AsRef<Path>) -> Self {
        let game_dir = game_dir.as_ref().to_path_buf();
        let cache_path = game_dir.join("patcher.dat");
        let lock_path = game_dir.join("patcher.lock");

        Self {
            config,
            game_dir,
            cache_path,
            lock_path,
            cancel_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_signal.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_signal.load(Ordering::SeqCst)
    }

    pub async fn run_update<F>(&self, mut status_cb: F) -> Result<PatchingResult>
    where
        F: FnMut(EngineStatus) + Send + Sync + 'static,
    {
        self.cancel_signal.store(false, Ordering::SeqCst);

        // Step 1: Acquire lock (with brief retry for self-update / Admin relaunch handoff)
        let _lock = FileLock::try_acquire_with_retry(
            &self.lock_path,
            10,
            std::time::Duration::from_millis(150),
        )
        .with_context(|| "Failed to acquire patcher lock")?;

        status_cb(EngineStatus::CheckingForUpdates);

        // Step 2: Read cache
        let cache = cache::read_cache_file(&self.cache_path).unwrap_or_default();
        log::info!("Current local patch index: {}", cache.last_patch_index);

        // Step 3: Find available server & fetch patch list
        let http_client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) ModernROPatcher/1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let (all_patches, patch_url) = find_available_patch_server(
            &http_client,
            &self.config.server.patch_servers,
            self.config.server.preferred_server.as_deref(),
        )
        .await?;

        // Filter patches that haven't been applied yet
        let pending: Vec<_> = all_patches
            .into_iter()
            .filter(|p| p.index > cache.last_patch_index)
            .collect();

        if pending.is_empty() {
            log::info!("All patches are up-to-date.");
            status_cb(EngineStatus::UpToDate);
            return Ok(PatchingResult {
                applied_count: 0,
                self_updated: false,
            });
        }

        log::info!("Found {} new patches to download", pending.len());

        // Step 4: Download patches to temporary folder
        let temp_dir = tempfile::tempdir_in(&self.game_dir)
            .or_else(|_| tempfile::tempdir())
            .with_context(|| "Failed to create temporary download directory")?;

        let status_cb_arc = Arc::new(std::sync::Mutex::new(status_cb));
        let cb_download = {
            let status_cb_clone = status_cb_arc.clone();
            move |prog: DownloadProgress| {
                if let Ok(mut lock) = status_cb_clone.lock() {
                    lock(EngineStatus::Downloading(prog));
                }
            }
        };

        let downloaded = download_patches_concurrent(
            &http_client,
            &patch_url,
            pending,
            temp_dir.path(),
            self.config.patching.check_integrity,
            self.config.patching.concurrent_downloads,
            self.cancel_signal.clone(),
            cb_download,
        )
        .await?;

        if self.is_cancelled() {
            bail!("Update cancelled by user.");
        }

        // Step 5: Install patches
        let mut status_cb_lock = status_cb_arc
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to lock status callback mutex"))?;
        let res = apply_patch_queue(
            downloaded,
            &self.config,
            &self.game_dir,
            &self.cache_path,
            |inst| {
                status_cb_lock(EngineStatus::Installing(inst));
            },
        )?;

        if res.self_updated {
            status_cb_lock(EngineStatus::SelfUpdated);
        } else {
            status_cb_lock(EngineStatus::UpToDate);
        }

        Ok(res)
    }

    pub fn launch_game(&self) -> Result<()> {
        launcher::launch_game(
            &self.config.client.executable,
            &self.config.client.arguments,
            &self.game_dir,
        )
    }

    pub fn launch_setup(&self) -> Result<()> {
        if let Some(ref setup) = self.config.client.setup_executable {
            launcher::launch_setup(setup, &self.game_dir)?;
        }
        Ok(())
    }

    pub fn restart_self(&self) -> Result<()> {
        launcher::restart_current_exe(&self.game_dir)
    }

    pub fn relaunch_as_admin(&self) -> Result<bool> {
        launcher::relaunch_as_admin(&self.game_dir)
    }

    pub fn is_running_as_admin(&self) -> bool {
        launcher::is_running_as_admin()
    }

    pub fn reset_cache(&self) -> Result<()> {
        cache::write_cache_file(
            &self.cache_path,
            &cache::PatcherCache {
                last_patch_index: 0,
                updated_at: None,
            },
        )
    }
}
