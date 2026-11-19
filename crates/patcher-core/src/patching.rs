use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use gruf::grf::{GrfArchive, GrfArchiveBuilder};
use gruf::thor::{ThorArchive, ThorFileEntry};

use crate::cache::{write_cache_file, PatcherCache};
use crate::config::PatcherConfig;
use crate::downloader::DownloadedPatch;
use crate::overlay::read_overlay_from_reader;

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

        let grf_path = resolve_safe_grf_path(game_dir, &target_grf)?;
        let method = if config.patching.in_place {
            GrfPatchingMethod::InPlace
        } else {
            GrfPatchingMethod::OutOfPlace
        };

        apply_patch_to_grf(method, config.patching.create_grf, &grf_path, &mut archive)?;
        Ok(false)
    } else {
        apply_patch_to_disk_with_integrity(game_dir, &mut archive, config.patching.check_integrity)
    }
}

/// Clears the Read-Only attribute on a file if it is currently set.
pub fn clear_readonly_if_needed(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
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
        let open_res = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(grf_path)
            .or_else(|_| {
                clear_readonly_if_needed(grf_path);
                fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(grf_path)
            });

        open_res.with_context(|| {
            format!(
                "Target GRF {:?} is locked, read-only, or requires Administrator privileges (please close the game client or switch to Admin Mode)",
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

        let mut ordered_entries: Vec<(String, MergeEntry)> = merge_entries.into_iter().collect();
        ordered_entries.sort_unstable_by_key(|(_, item)| (!item.is_grf, item.offset));

        let file = BufWriter::with_capacity(256 * 1024, File::create(grf_path)?);
        let mut builder = GrfArchiveBuilder::create(file, 2, 0)?;
        for (rel_path, item) in ordered_entries {
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
    apply_patch_to_disk_with_integrity(game_dir, thor, false)
}

pub fn apply_patch_to_disk_with_integrity<R: Read + Seek>(
    game_dir: &Path,
    thor: &mut ThorArchive<R>,
    require_self_update_integrity: bool,
) -> Result<bool> {
    let mut entries: Vec<ThorFileEntry> = thor
        .get_entries()
        .filter(|e| !e.is_internal())
        .cloned()
        .collect();

    entries.sort_unstable_by_key(|e| e.offset);

    let current_exe = std::env::current_exe().ok();
    let current_exe_canon = current_exe
        .as_ref()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()));
    let current_has_overlay = current_exe
        .as_ref()
        .and_then(|p| File::open(p).ok())
        .and_then(|mut f| read_overlay_from_reader(&mut f).ok().flatten())
        .is_some();

    let mut self_updated = false;

    for entry in entries {
        let is_dir_entry = entry.relative_path.ends_with('\\') || entry.relative_path.ends_with('/');
        let dest_path = resolve_safe_relative_path(game_dir, &entry.relative_path)
            .with_context(|| format!("Unsafe patch entry path {:?}", entry.relative_path))?;

        if is_dir_entry || dest_path.is_dir() {
            if !entry.is_removed {
                fs::create_dir_all(&dest_path)?;
            }
            continue;
        }

        if entry.is_removed {
            if fs::remove_file(&dest_path).is_err() {
                clear_readonly_if_needed(&dest_path);
                let _ = fs::remove_file(&dest_path);
            }
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Check for Self-Update: require BOTH same filename (case-insensitive) AND same directory
        let is_current_exe = match (
            current_exe_canon
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str()),
            current_exe_canon.as_ref().and_then(|p| p.parent()),
            dest_path.file_name().and_then(|n| n.to_str()),
            dest_path.parent(),
        ) {
            (Some(cur_name), Some(cur_dir), Some(target_name), Some(target_dir)) => {
                let target_dir_canon =
                    fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
                cur_name.eq_ignore_ascii_case(target_name) && cur_dir == target_dir_canon
            }
            _ => false,
        };

        if is_current_exe {
            if require_self_update_integrity && !thor.is_valid().unwrap_or(false) {
                bail!(
                    "Refusing self-update from {:?}: THOR archive must include a valid data.integrity record.",
                    entry.relative_path
                );
            }

            let mut staged_os = dest_path.as_os_str().to_os_string();
            staged_os.push(".update.tmp");
            let staged_path = PathBuf::from(staged_os);
            clear_readonly_if_needed(&staged_path);
            let _ = fs::remove_file(&staged_path);

            if let Err(err) = thor.extract_file(&entry.relative_path, &staged_path) {
                let _ = fs::remove_file(&staged_path);
                return Err(err)
                    .with_context(|| format!("Failed to stage self-update {:?}", staged_path));
            }

            if let Err(err) = validate_staged_self_update(&staged_path, current_has_overlay) {
                let _ = fs::remove_file(&staged_path);
                return Err(err);
            }

            let mut backup_exe = dest_path.clone();
            backup_exe.set_extension("exe.old");
            clear_readonly_if_needed(&backup_exe);
            let _ = fs::remove_file(&backup_exe);

            let had_existing = dest_path.exists();
            if had_existing {
                clear_readonly_if_needed(&dest_path);
                if let Err(err) = fs::rename(&dest_path, &backup_exe) {
                    let _ = fs::remove_file(&staged_path);
                    return Err(err).with_context(|| {
                        format!("Failed backing up running patcher to {:?}", backup_exe)
                    });
                }
            }

            if let Err(err) = fs::rename(&staged_path, &dest_path) {
                if had_existing {
                    let _ = fs::rename(&backup_exe, &dest_path);
                }
                let _ = fs::remove_file(&staged_path);
                return Err(err).with_context(|| {
                    format!("Failed replacing patcher executable {:?}", dest_path)
                });
            }

            self_updated = true;
            continue;
        }

        if thor.extract_file(&entry.relative_path, &dest_path).is_err() {
            clear_readonly_if_needed(&dest_path);
            thor.extract_file(&entry.relative_path, &dest_path)
                .with_context(|| format!("Failed to write file {:?}", dest_path))?;
        }
    }

    Ok(self_updated)
}

fn validate_staged_self_update(staged_path: &Path, require_overlay: bool) -> Result<()> {
    let mut file = File::open(staged_path)
        .with_context(|| format!("Failed opening staged self-update {:?}", staged_path))?;
    let mut mz = [0u8; 2];
    file.read_exact(&mut mz)
        .with_context(|| "Staged self-update is truncated (missing MZ header)")?;
    if &mz != b"MZ" {
        bail!("Staged self-update is not a valid Windows PE executable (invalid MZ signature).");
    }

    file.seek(SeekFrom::Start(0x3C))?;
    let mut e_lfanew_buf = [0u8; 4];
    file.read_exact(&mut e_lfanew_buf)
        .with_context(|| "Staged self-update is truncated (missing e_lfanew)")?;
    let pe_offset = u32::from_le_bytes(e_lfanew_buf) as u64;
    let file_len = file.metadata()?.len();
    if pe_offset.checked_add(4).map_or(true, |end| end > file_len) {
        bail!("Staged self-update has invalid PE header offset.");
    }

    file.seek(SeekFrom::Start(pe_offset))?;
    let mut pe_sig = [0u8; 4];
    file.read_exact(&mut pe_sig)?;
    if &pe_sig != b"PE\0\0" {
        bail!("Staged self-update is not a valid Windows PE executable (missing PE signature).");
    }

    if require_overlay {
        file.seek(SeekFrom::Start(0))?;
        match read_overlay_from_reader(&mut file) {
            Ok(Some(_)) => {}
            Ok(None) => {
                bail!("Staged self-update executable is missing required ROPATCH1 overlay.");
            }
            Err(e) => {
                bail!("Staged self-update executable failed overlay integrity check: {:#}", e);
            }
        }
    }

    Ok(())
}

pub(crate) fn is_windows_reserved_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment).trim();
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Validates that `rel` is a safe relative path (no traversal, UNC, drive/ADS `:`,
/// control characters, trailing dots/spaces, or Windows reserved device names) and
/// returns its normalized components.
pub fn validate_relative_patch_path(rel: &str) -> Result<Vec<String>> {
    if rel.trim().is_empty() {
        bail!("Relative path cannot be empty");
    }
    if rel.chars().any(|c| c.is_control()) {
        bail!("Relative path contains control characters: {:?}", rel);
    }
    if rel.starts_with("\\\\")
        || rel.starts_with("//")
        || rel.starts_with("\\/")
        || rel.starts_with("/\\")
    {
        bail!("UNC or network paths are not allowed: {:?}", rel);
    }
    if rel.contains(':') {
        bail!("Drive letters or alternate data streams (':') are not allowed: {:?}", rel);
    }

    // Allow at most a single leading separator for legacy THOR compatibility
    let stripped = rel
        .strip_prefix('\\')
        .or_else(|| rel.strip_prefix('/'))
        .unwrap_or(rel);

    let mut components = Vec::new();
    for raw_part in stripped.split('\\').flat_map(|p| p.split('/')) {
        if raw_part.is_empty() || raw_part == "." {
            continue;
        }
        if raw_part == ".." {
            bail!("Path traversal ('..') is not allowed: {:?}", rel);
        }
        if raw_part.ends_with('.') || raw_part.ends_with(' ') {
            bail!(
                "Path segment ending with dot or space is not allowed: {:?}",
                raw_part
            );
        }
        if is_windows_reserved_device_name(raw_part) {
            bail!("Windows reserved device name is not allowed: {:?}", raw_part);
        }
        components.push(raw_part.to_string());
    }

    if components.is_empty() {
        bail!("Relative path resolved to empty path: {:?}", rel);
    }

    Ok(components)
}

/// Resolves a validated relative patch path under `root`, enforcing symlink/junction
/// containment when `root` exists on disk.
pub fn resolve_safe_relative_path(root: &Path, rel: &str) -> Result<PathBuf> {
    let components = validate_relative_patch_path(rel)?;
    let mut joined = PathBuf::from(root);
    for part in &components {
        joined.push(part);
    }

    if root.exists() {
        let canon_root = fs::canonicalize(root)
            .with_context(|| format!("Failed to canonicalize root directory {:?}", root))?;

        let mut probe = joined.as_path();
        loop {
            if probe.exists() {
                let canon_probe = fs::canonicalize(probe).with_context(|| {
                    format!("Failed to canonicalize path prefix {:?}", probe)
                })?;
                if !canon_probe.starts_with(&canon_root) {
                    bail!(
                        "Resolved path {:?} escapes root directory {:?}",
                        joined,
                        root
                    );
                }
                break;
            }
            if fs::symlink_metadata(probe)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                bail!("Symlink target {:?} could not be verified inside root {:?}", probe, root);
            }
            if probe == root {
                break;
            }
            match probe.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => probe = parent,
                _ => break,
            }
        }
    }

    Ok(joined)
}

/// Resolves a target GRF archive path safely within `game_dir`.
pub fn resolve_safe_grf_path(game_dir: &Path, target_grf: &str) -> Result<PathBuf> {
    resolve_safe_relative_path(game_dir, target_grf)
        .with_context(|| format!("Invalid target GRF path {:?}", target_grf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gruf::thor::ThorArchiveBuilder;
    use std::io::Cursor;

    #[test]
    fn test_join_relative_path() {
        let root = Path::new("D:/games/ro");
        let path = resolve_safe_relative_path(root, "System\\itemInfo.lub").unwrap();
        assert_eq!(path, Path::new("D:/games/ro/System/itemInfo.lub"));

        let leading = resolve_safe_relative_path(root, "\\data\\sprite\\a.spr").unwrap();
        assert_eq!(leading, Path::new("D:/games/ro/data/sprite/a.spr"));
    }

    #[test]
    fn test_join_relative_path_traversal_and_unsafe_blocked() {
        let root = Path::new("D:/games/ro");
        for bad in [
            "",
            "   ",
            ".",
            "\\",
            "/",
            "..\\..\\Windows\\System32\\evil.dll",
            "data\\..\\..\\evil.dll",
            "C:\\Users\\Public\\hack.exe",
            "data\\file.txt:ads",
            "\\\\server\\share\\evil.dll",
            "//server/share/evil.dll",
            "CON",
            "data\\nul.txt",
            "System\\COM1",
            "System\\LPT9.lub",
            "data\\trailing_dot.",
            "data\\trailing_space ",
            "data\\bad\0name.txt",
            "data\\bad\nname.txt",
        ] {
            assert!(
                resolve_safe_relative_path(root, bad).is_err(),
                "Expected {:?} to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_apply_single_patch_rejects_target_grf_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let game_dir = dir.path();
        let thor_path = game_dir.join("malicious_grf.thor");
        {
            let file = File::create(&thor_path).unwrap();
            let mut builder = ThorArchiveBuilder::new(
                file,
                true,
                Some("..\\..\\escape.grf".to_string()),
                false,
            )
            .unwrap();
            builder
                .append_file_update("data\\test.txt".to_string(), Cursor::new(b"x"))
                .unwrap();
        }

        let config = PatcherConfig::default();
        let res = apply_single_patch(&thor_path, &config, game_dir);
        assert!(res.is_err());
        assert!(!game_dir.join("..\\..\\escape.grf").exists());
    }
}
