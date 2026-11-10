use std::fs::File;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};
use gruf::thor::ThorArchiveBuilder;
use patcher_core::plist::{format_patch_list, parse_patch_list, PatchItem};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilenameEncoding {
    Unicode,
    Ascii,
}

impl Default for FilenameEncoding {
    fn default() -> Self {
        Self::Unicode
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThorBuildOptions {
    pub use_grf_merging: bool,
    pub target_grf_name: Option<String>,
    #[serde(default = "default_true")]
    pub include_checksums: bool,
    #[serde(default = "default_compression_level")]
    pub compression_level: u32,
    #[serde(default)]
    pub filename_encoding: FilenameEncoding,
}

fn default_true() -> bool {
    true
}

fn default_compression_level() -> u32 {
    9
}

impl Default for ThorBuildOptions {
    fn default() -> Self {
        Self {
            use_grf_merging: true,
            target_grf_name: Some("data.grf".to_string()),
            include_checksums: true,
            compression_level: 9,
            filename_encoding: FilenameEncoding::Unicode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEntry {
    /// Target path inside GRF or relative to game root (e.g. "data\\sprite\\item.spr" or "System\\itemInfo.lub")
    pub target_relative_path: String,
    /// If true, entry will be deleted from target GRF or disk
    #[serde(default)]
    pub is_removed: bool,
    /// Absolute or relative path to the source file on disk (not required if is_removed is true)
    pub source_path: Option<PathBuf>,
}

pub fn build_thor_patch(
    output_path: impl AsRef<Path>,
    options: &ThorBuildOptions,
    entries: &[PatchEntry],
) -> Result<()> {
    let out = output_path.as_ref();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(out)
        .with_context(|| format!("Failed to create output THOR archive at {:?}", out))?;

    let mut builder = ThorArchiveBuilder::new(
        file,
        options.use_grf_merging,
        options.target_grf_name.clone(),
        options.include_checksums,
    )?
    .with_compression_level(options.compression_level);

    for entry in entries {
        let win_path = to_win32_path(&entry.target_relative_path);

        if entry.is_removed {
            log::trace!("Adding file removal for '{}'", win_path);
            builder.append_file_removal(win_path);
            continue;
        }

        let source = entry.source_path.as_ref().ok_or_else(|| {
            anyhow!("Source path is required for non-removal entry: '{}'", win_path)
        })?;

        if !source.exists() || !source.is_file() {
            return Err(anyhow!("Source file does not exist or is not a file: {:?}", source));
        }

        log::trace!("Adding file update: '{}' from {:?}", win_path, source);
        let src_file = File::open(source)
            .with_context(|| format!("Failed to open source file {:?}", source))?;
        builder.append_file_update(win_path, src_file)?;
    }

    builder
        .finish()
        .with_context(|| format!("Failed to finalize THOR archive table at {:?}", out))?;

    Ok(())
}

fn is_system_junk_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        matches!(lower.as_str(), "thumbs.db" | "desktop.ini" | ".ds_store")
    } else {
        false
    }
}

pub fn collect_entries_from_directory(directory_path: impl AsRef<Path>) -> Result<Vec<PatchEntry>> {
    let root = directory_path.as_ref();
    if !root.exists() || !root.is_dir() {
        return Err(anyhow!("Directory does not exist: {:?}", root));
    }

    let mut entries = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && !is_system_junk_file(path) {
            let rel = path.strip_prefix(root)?;
            let rel_str = rel.to_string_lossy().to_string();
            entries.push(PatchEntry {
                target_relative_path: rel_str,
                is_removed: false,
                source_path: Some(path.to_path_buf()),
            });
        }
    }
    Ok(entries)
}

/// Recursively scans a directory and packages all files into a single .thor patch.
pub fn build_thor_from_directory(
    directory_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: &ThorBuildOptions,
) -> Result<usize> {
    let entries = collect_entries_from_directory(directory_path)?;
    let count = entries.len();
    build_thor_patch(output_path, options, &entries)?;
    Ok(count)
}

/// Packages a single file into a .thor patch, optionally overriding its target relative path.
pub fn build_thor_from_single_file(
    source_file_path: impl AsRef<Path>,
    target_relative_path: Option<String>,
    output_path: impl AsRef<Path>,
    options: &ThorBuildOptions,
) -> Result<usize> {
    let src = source_file_path.as_ref();
    if !src.exists() || !src.is_file() {
        return Err(anyhow!("Source file does not exist: {:?}", src));
    }

    let target_rel = target_relative_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            src.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let entries = vec![PatchEntry {
        target_relative_path: target_rel,
        is_removed: false,
        source_path: Some(src.to_path_buf()),
    }];

    build_thor_patch(output_path, options, &entries)?;
    Ok(1)
}

/// Parses a multiline list of file paths to delete (from a .txt file or clipboard).
pub fn parse_delete_list(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let normalized = to_win32_path(trimmed);
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    out
}

/// Appends a new patch to a plist.txt file.
/// If index is None, auto-increments based on the highest existing index + 1 (starts at 1).
pub fn add_patch_to_plist(
    plist_path: impl AsRef<Path>,
    thor_file_name: &str,
    forced_index: Option<u32>,
) -> Result<u32> {
    let path = plist_path.as_ref();

    let mut list = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        parse_patch_list(&content)
    } else {
        Vec::new()
    };

    let next_index = match forced_index {
        Some(idx) => idx,
        None => list.iter().map(|p| p.index).max().unwrap_or(0) + 1,
    };

    // Remove any existing entry with the same index
    list.retain(|p| p.index != next_index);
    list.push(PatchItem {
        index: next_index,
        file_name: thor_file_name.to_string(),
    });
    list.sort_by_key(|p| p.index);

    let formatted = format_patch_list(&list);
    std::fs::write(path, formatted)
        .with_context(|| format!("Failed to update plist file at {:?}", path))?;

    Ok(next_index)
}

fn to_win32_path(path: &str) -> String {
    path.replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gruf::thor::ThorArchive;
    use tempfile::tempdir;

    #[test]
    fn test_patch_maker_from_dir() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path().join("patch_data");
        std::fs::create_dir_all(data_dir.join("data/sprite")).unwrap();
        std::fs::write(data_dir.join("data/sprite/test.txt"), b"Sprite Data").unwrap();
        std::fs::write(data_dir.join("data/item.txt"), b"Item Data").unwrap();

        let output_thor = temp.path().join("001_patch.thor");
        let options = ThorBuildOptions {
            use_grf_merging: true,
            target_grf_name: Some("data.grf".to_string()),
            include_checksums: true,
            ..Default::default()
        };

        let count = build_thor_from_directory(&data_dir, &output_thor, &options).unwrap();
        assert_eq!(count, 2);
        assert!(output_thor.exists());

        // Validate generated THOR
        let archive = ThorArchive::open(&output_thor).unwrap();
        assert!(archive.use_grf_merging());
        assert_eq!(archive.target_grf_name(), "data.grf");
        // Count non-internal files
        let non_internal_count = archive.get_entries().filter(|e| !e.is_internal()).count();
        assert_eq!(non_internal_count, 2);
    }

    #[test]
    fn test_plist_manager() {
        let temp = tempdir().unwrap();
        let plist_file = temp.path().join("plist.txt");

        let idx1 = add_patch_to_plist(&plist_file, "001.thor", None).unwrap();
        assert_eq!(idx1, 1);

        let idx2 = add_patch_to_plist(&plist_file, "002.thor", None).unwrap();
        assert_eq!(idx2, 2);

        let content = std::fs::read_to_string(&plist_file).unwrap();
        assert!(content.contains("1 001.thor"));
        assert!(content.contains("2 002.thor"));
    }
}
