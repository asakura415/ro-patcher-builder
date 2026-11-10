use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gruf::grf::{GrfArchive, GrfArchiveBuilder};
use gruf::thor::{ThorArchive, ThorFileEntry};

use crate::cache::{write_cache_file, PatcherCache};
use crate::config::PatcherConfig;
use crate::downloader::DownloadedPatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrfPatchingMethod {
    InPlace,
    OutOfPlace,
}

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub installed_patches: usize,
    pub total_patches: usize,
    pub current_patch_name: String,
}

pub struct PatchingResult {
    pub applied_count: usize,
    pub self_updated: bool,
}

/// Applies a sequence of downloaded patches, recording progress to the cache file.
pub fn apply_patch_queue<F>(
    patches: Vec<DownloadedPatch>,
    config: &PatcherConfig,
    game_dir: &Path,
    cache_path: &Path,
    mut progress_cb: F,
) -> Result<PatchingResult>
where
    F: FnMut(InstallProgress),
{
    let total = patches.len();
    let mut self_updated = false;
    let mut applied_count = 0;

    for (idx, patch) in patches.into_iter().enumerate() {
        let patch_name = patch.info.file_name.clone();
        progress_cb(InstallProgress {
            installed_patches: idx,
            total_patches: total,
            current_patch_name: patch_name.clone(),
        });

        let updated = apply_single_patch(&patch.local_path, config, game_dir)
            .with_context(|| format!("Failed applying patch {}", patch_name))?;

        if updated {
            self_updated = true;
        }

        // Immediately update cache file after every successful patch
        let new_cache = PatcherCache {
            last_patch_index: patch.info.index,
            updated_at: Some(chrono_now()),
        };
        let _ = write_cache_file(cache_path, &new_cache);

        applied_count += 1;
    }

    progress_cb(InstallProgress {
        installed_patches: total,
        total_patches: total,
        current_patch_name: String::new(),
    });

    Ok(PatchingResult {
        applied_count,
        self_updated,
    })
}

fn chrono_now() -> String {
    // Simple timestamp without heavy chrono dep
    format!("{:?}", std::time::SystemTime::now())
}

/// Applies a single .thor patch to either the target GRF or the game directory.
/// Returns true if the patch contained a self-update for the current executable.
pub fn apply_single_patch(
    thor_path: &Path,
    config: &PatcherConfig,
    game_dir: &Path,
) -> Result<bool> {
    let mut archive = ThorArchive::open(thor_path)
        .with_context(|| format!("Failed to open THOR archive {:?}", thor_path))?;

    if archive.use_grf_merging() {
        let target_grf = if archive.target_grf_name().is_empty() {
            config.client.default_grf_name.clone()
        } else {
            archive.target_grf_name()
        };

        let grf_path = game_dir.join(&target_grf);
        let method = if config.patching.in_place {
            GrfPatchingMethod::InPlace
        } else {
            GrfPatchingMethod::OutOfPlace
        };

        apply_patch_to_grf(method, config.patching.create_grf, &grf_path, &mut archive)?;
        Ok(false)
    } else {
        apply_patch_to_disk(game_dir, &mut archive)
    }
}

pub fn apply_patch_to_grf<R: Read + Seek>(
    method: GrfPatchingMethod,
    create_if_needed: bool,
    grf_path: &Path,
    thor: &mut ThorArchive<R>,
) -> Result<()> {
    if !grf_path.exists() && create_if_needed {
        if let Some(parent) = grf_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(grf_path)?;
        let mut builder = GrfArchiveBuilder::create(file, 2, 0)?;
        builder.finish()?;
    }

    if grf_path.exists() {
        // Verify that the GRF file is writable and not locked by a running game client
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(grf_path)
            .with_context(|| {
                format!(
                    "Target GRF {:?} is locked or in use by another process (please close the game client first)",
                    grf_path.file_name().unwrap_or_default()
                )
            })?;
    }

    match method {
        GrfPatchingMethod::InPlace => apply_patch_to_grf_in_place(grf_path, thor),
        GrfPatchingMethod::OutOfPlace => apply_patch_to_grf_out_of_place(grf_path, thor),
    }
}

fn apply_patch_to_grf_in_place<R: Read + Seek>(
    grf_path: &Path,
    thor: &mut ThorArchive<R>,
) -> Result<()> {
    let mut builder = GrfArchiveBuilder::open(grf_path)?;
    let mut entries: Vec<ThorFileEntry> = thor
        .get_entries()
        .filter(|e| !e.is_internal())
        .cloned()
        .collect();

    entries.sort_unstable_by_key(|e| e.offset);

    for entry in entries {
        if entry.is_removed {
            let _ = builder.remove_file(&entry.relative_path);
        } else {
            builder.import_raw_entry_from_thor(thor, entry.relative_path)?;
        }
    }
    builder
        .finish()
        .with_context(|| format!("Failed to finalize GRF table in {:?}", grf_path))?;
    Ok(())
}

fn apply_patch_to_grf_out_of_place<R: Read + Seek>(
    grf_path: &Path,
    thor: &mut ThorArchive<R>,
) -> Result<()> {
    let mut backup_path = grf_path.to_path_buf();
    backup_path.set_extension("grf.bak");
    fs::rename(grf_path, &backup_path)?;

    let merge_res = (|| -> Result<()> {
        #[allow(dead_code)]
        struct MergeEntry {
            is_grf: bool,
            offset: u64,
            size_compressed: usize,
        }

        let mut merge_entries: HashMap<String, MergeEntry> = HashMap::new();
        let mut grf_archive = GrfArchive::open(&backup_path)?;

        for entry in grf_archive.get_entries() {
            if let Some(thor_entry) = thor.get_file_entry(&entry.relative_path) {
                if thor_entry.is_removed {
                    continue;
                }
            }
            merge_entries.insert(
                entry.relative_path.clone(),
                MergeEntry {
                    is_grf: true,
                    offset: entry.offset,
                    size_compressed: entry.size_compressed,
                },
            );
        }

        for entry in thor.get_entries() {
            if entry.is_removed || entry.is_internal() {
                continue;
            }
            merge_entries.insert(
                entry.relative_path.clone(),
                MergeEntry {
                    is_grf: false,
                    offset: entry.offset,
                    size_compressed: entry.size_compressed,
                },
            );
        }

        let file = File::create(grf_path)?;
        let mut builder = GrfArchiveBuilder::create(file, 2, 0)?;
        for (rel_path, item) in merge_entries {
            if item.is_grf {
                builder.import_raw_entry_from_grf(&mut grf_archive, rel_path)?;
            } else {
                builder.import_raw_entry_from_thor(thor, rel_path)?;
            }
        }
        builder.finish()?;
        Ok(())
    })();

    if let Err(err) = merge_res {
        // Rollback backup GRF if out-of-place merge failed
        let _ = fs::remove_file(grf_path);
        let _ = fs::rename(&backup_path, grf_path);
        return Err(err);
    }

    let _ = fs::remove_file(backup_path);
    Ok(())
}

pub fn apply_patch_to_disk<R: Read + Seek>(
    game_dir: &Path,
    thor: &mut ThorArchive<R>,
) -> Result<bool> {
    let mut entries: Vec<ThorFileEntry> = thor
        .get_entries()
        .filter(|e| !e.is_internal())
        .cloned()
        .collect();

    entries.sort_unstable_by_key(|e| e.offset);

    let current_exe = std::env::current_exe().ok();
    let current_exe_name = current_exe
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase());

    let mut self_updated = false;

    for entry in entries {
        let dest_path = join_windows_relative_path(game_dir, &entry.relative_path);

        if entry.is_removed {
            let _ = fs::remove_file(&dest_path);
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Check for Self-Update: Is this file updating the current running executable?
        let is_current_exe = match (&current_exe_name, dest_path.file_name().and_then(|n| n.to_str())) {
            (Some(cur), Some(target)) => cur == &target.to_lowercase(),
            _ => false,
        };

        if is_current_exe {
            // Windows allows renaming a running executable!
            let mut backup_exe = dest_path.clone();
            backup_exe.set_extension("exe.old");
            let _ = fs::remove_file(&backup_exe); // Remove any old backup
            let _ = fs::rename(&dest_path, &backup_exe);
            self_updated = true;
        }

        thor.extract_file(&entry.relative_path, &dest_path)?;
    }

    Ok(self_updated)
}

fn join_windows_relative_path(root: &Path, rel: &str) -> PathBuf {
    let mut path = PathBuf::from(root);
    for part in rel.split('\\').flat_map(|p| p.split('/')) {
        let part = part.trim();
        // Prevent path traversal ("..") and drive prefix injection ("C:")
        if !part.is_empty() && part != "." && part != ".." && !part.contains(':') {
            path.push(part);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_relative_path() {
        let root = Path::new("D:/games/ro");
        let path = join_windows_relative_path(root, "System\\itemInfo.lub");
        assert_eq!(path, Path::new("D:/games/ro/System/itemInfo.lub"));
    }

    #[test]
    fn test_join_relative_path_traversal_blocked() {
        let root = Path::new("D:/games/ro");
        let path = join_windows_relative_path(root, "..\\..\\Windows\\System32\\evil.dll");
        assert_eq!(path, Path::new("D:/games/ro/Windows/System32/evil.dll"));

        let path_drive = join_windows_relative_path(root, "C:\\Users\\Public\\hack.exe");
        assert_eq!(path_drive, Path::new("D:/games/ro/Users/Public/hack.exe"));
    }
}
