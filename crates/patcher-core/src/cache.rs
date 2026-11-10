use std::path::Path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PatcherCache {
    pub last_patch_index: u32,
    #[serde(default)]
    pub updated_at: Option<String>,
}

pub fn read_cache_file(path: impl AsRef<Path>) -> Result<PatcherCache> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(PatcherCache::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read cache file at {:?}", path))?;

    // Try parsing as JSON first
    if let Ok(cache) = serde_json::from_str::<PatcherCache>(&content) {
        return Ok(cache);
    }

    // Fallback: simple integer on first line (backward compatibility)
    if let Some(first_line) = content.lines().next() {
        if let Ok(index) = first_line.trim().parse::<u32>() {
            return Ok(PatcherCache {
                last_patch_index: index,
                updated_at: None,
            });
        }
    }

    Ok(PatcherCache::default())
}

pub fn write_cache_file(path: impl AsRef<Path>, cache: &PatcherCache) -> Result<()> {
    let path = path.as_ref();
    let content = serde_json::to_string_pretty(cache)?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write cache file at {:?}", path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_cache_read_write() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let cache = PatcherCache {
            last_patch_index: 42,
            updated_at: Some("2026-11-10".to_string()),
        };

        write_cache_file(path, &cache).unwrap();
        let loaded = read_cache_file(path).unwrap();
        assert_eq!(loaded, cache);
    }
}
