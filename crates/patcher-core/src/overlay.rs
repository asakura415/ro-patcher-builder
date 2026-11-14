use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::config::PatcherConfig;

pub const OVERLAY_MAGIC: &[u8; 8] = b"ROPATCH1";
pub const TRAILER_SIZE: usize = 48; // 8 bytes (size) + 32 bytes (sha256) + 8 bytes (magic)

#[derive(Debug, Clone)]
pub struct EmbeddedAssets {
    pub config: PatcherConfig,
    /// Maps normalized relative paths (e.g. "index.html", "assets/logo.png") to (file_bytes, mime_type)
    pub files: HashMap<String, (Vec<u8>, String)>,
}

/// Computes SHA-256 hash of arbitrary byte slice
pub fn compute_sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Reads the binary overlay payload from any Read + Seek stream.
/// Returns Ok(None) if the magic signature is not found at the end of the stream.
pub fn read_overlay_from_reader<R: Read + Seek>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let stream_len = reader.seek(SeekFrom::End(0))?;
    if stream_len < TRAILER_SIZE as u64 {
        return Ok(None);
    }

    // Read the 48-byte trailer from EOF
    reader.seek(SeekFrom::End(-(TRAILER_SIZE as i64)))?;
    let mut trailer = [0u8; TRAILER_SIZE];
    reader.read_exact(&mut trailer)?;

    // Layout: [Payload Size: 8 bytes] [SHA-256: 32 bytes] [Magic: 8 bytes]
    let size_bytes: [u8; 8] = trailer[0..8].try_into()?;
    let expected_sha256: [u8; 32] = trailer[8..40].try_into()?;
    let magic: [u8; 8] = trailer[40..48].try_into()?;

    if &magic != OVERLAY_MAGIC {
        return Ok(None);
    }

    let payload_size = u64::from_le_bytes(size_bytes);
    if stream_len < (TRAILER_SIZE as u64) + payload_size {
        bail!("Invalid overlay: declared payload size exceeds file size.");
    }

    // Seek to beginning of payload
    let payload_offset = stream_len - (TRAILER_SIZE as u64) - payload_size;
    reader.seek(SeekFrom::Start(payload_offset))?;

    let mut payload = vec![0u8; payload_size as usize];
    reader.read_exact(&mut payload)?;

    // Verify SHA-256
    let actual_sha256 = compute_sha256(&payload);
    if actual_sha256 != expected_sha256 {
        bail!("Corrupted overlay payload: SHA-256 checksum mismatch.");
    }

    Ok(Some(payload))
}

/// Reads overlay payload from the currently executing binary.
pub fn read_overlay_from_current_exe() -> Result<Option<Vec<u8>>> {
    let exe_path = std::env::current_exe()
        .with_context(|| "Failed to get current executable path")?;
    let mut file = File::open(&exe_path)
        .with_context(|| format!("Failed to open current executable {:?}", exe_path))?;
    read_overlay_from_reader(&mut file)
}

/// Writes a payload + trailer onto the end of an existing executable file.
pub fn append_overlay_to_file(target_file_path: impl AsRef<Path>, payload: &[u8]) -> Result<()> {
    let target = target_file_path.as_ref();
    let mut file = OpenOptions::new()
        .write(true)
        .open(target)
        .with_context(|| format!("Failed to open target executable {:?}", target))?;

    // Seek to the end of the file
    file.seek(SeekFrom::End(0))?;

    // 1. Write ZIP Payload
    file.write_all(payload)?;

    // 2. Write 48-byte Trailer
    let payload_size = (payload.len() as u64).to_le_bytes();
    let sha256_digest = compute_sha256(payload);

    file.write_all(&payload_size)?;
    file.write_all(&sha256_digest)?;
    file.write_all(OVERLAY_MAGIC)?;

    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// Builds an in-memory ZIP package containing `config.json` and all web frontend files.
pub fn create_overlay_zip(config: &PatcherConfig, web_folder: impl AsRef<Path>) -> Result<Vec<u8>> {
    let web_path = web_folder.as_ref();
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let deflated_options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored_options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    // 1. Write config.json
    zip.start_file("config.json", deflated_options)?;
    let config_json = config.to_json_string()?;
    zip.write_all(config_json.as_bytes())?;

    // 2. Write web directory files
    if web_path.exists() && web_path.is_dir() {
        for entry in WalkDir::new(web_path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(rel_path) = path.strip_prefix(web_path) {
                    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                    if rel_str == "config.json" {
                        continue; // Do not overwrite explicit config
                    }

                    // Compressed audio files (mp3/ogg) don't benefit from deflate and stream faster Stored
                    let lower_rel = rel_str.to_ascii_lowercase();
                    let entry_options = if lower_rel.ends_with(".mp3") || lower_rel.ends_with(".ogg") {
                        stored_options
                    } else {
                        deflated_options
                    };

                    zip.start_file(&rel_str, entry_options)?;
                    let mut file = File::open(path)?;
                    let mut content = Vec::new();
                    file.read_to_end(&mut content)?;
                    zip.write_all(&content)?;
                }
            }
        }
    }

    zip.finish()?;
    Ok(buffer.into_inner())
}

/// Extracts assets and configuration from in-memory ZIP payload.
pub fn parse_embedded_assets(payload_bytes: &[u8]) -> Result<EmbeddedAssets> {
    let mut archive = ZipArchive::new(Cursor::new(payload_bytes))
        .with_context(|| "Failed to parse overlay ZIP container")?;

    let mut config = PatcherConfig::default();
    let mut files = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().replace('\\', "/");
        if file.is_dir() {
            continue;
        }

        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)?;

        if name == "config.json" {
            if let Ok(cfg_str) = String::from_utf8(data.clone()) {
                if let Ok(parsed_cfg) = PatcherConfig::from_json_str(&cfg_str) {
                    config = parsed_cfg;
                }
            }
        }

        let mime = guess_mime_type(&name);
        files.insert(name.clone(), (data, mime));
    }

    Ok(EmbeddedAssets { config, files })
}

pub fn guess_mime_type(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8".to_string()
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8".to_string()
    } else if lower.ends_with(".js") {
        "application/javascript; charset=utf-8".to_string()
    } else if lower.ends_with(".json") {
        "application/json".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if lower.ends_with(".ico") {
        "image/x-icon".to_string()
    } else if lower.ends_with(".woff2") {
        "font/woff2".to_string()
    } else if lower.ends_with(".woff") {
        "font/woff".to_string()
    } else if lower.ends_with(".ttf") {
        "font/ttf".to_string()
    } else if lower.ends_with(".mp3") {
        "audio/mpeg".to_string()
    } else if lower.ends_with(".ogg") {
        "audio/ogg".to_string()
    } else if lower.ends_with(".wav") {
        "audio/wav".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_overlay_roundtrip() {
        let temp_exe = NamedTempFile::new().unwrap();
        let path = temp_exe.path();

        // Write dummy PE content
        std::fs::write(path, b"MZ......Fake PE File Content......").unwrap();

        let mut config = PatcherConfig::default();
        config.client.executable = "my_custom_client.exe".to_string();

        let temp_web_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_web_dir.path().join("index.html"), b"<h1>Hello RO</h1>").unwrap();
        std::fs::write(temp_web_dir.path().join("style.css"), b"body { color: red; }").unwrap();

        let payload = create_overlay_zip(&config, temp_web_dir.path()).unwrap();
        append_overlay_to_file(path, &payload).unwrap();

        // Now read it back
        let mut file = File::open(path).unwrap();
        let read_payload = read_overlay_from_reader(&mut file).unwrap();
        assert!(read_payload.is_some());

        let assets = parse_embedded_assets(&read_payload.unwrap()).unwrap();
        assert_eq!(assets.config.client.executable, "my_custom_client.exe");
        assert!(assets.files.contains_key("index.html"));
        assert!(assets.files.contains_key("style.css"));
        assert_eq!(assets.files["index.html"].0, b"<h1>Hello RO</h1>");
    }

    #[test]
    fn test_overlay_with_bgm_mp3_and_without() {
        let config = PatcherConfig::default();

        // Case 1: Theme with assets/bgm.mp3
        let theme_with_bgm = tempfile::tempdir().unwrap();
        std::fs::write(theme_with_bgm.path().join("index.html"), b"<h1>Sakura</h1>").unwrap();
        std::fs::create_dir_all(theme_with_bgm.path().join("assets")).unwrap();
        let fake_mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x00FAKE_MP3_PAYLOAD";
        std::fs::write(theme_with_bgm.path().join("assets/bgm.mp3"), fake_mp3).unwrap();

        let payload_with = create_overlay_zip(&config, theme_with_bgm.path()).unwrap();
        let assets_with = parse_embedded_assets(&payload_with).unwrap();
        assert!(assets_with.files.contains_key("assets/bgm.mp3"));
        assert_eq!(assets_with.files["assets/bgm.mp3"].0, fake_mp3);
        assert_eq!(assets_with.files["assets/bgm.mp3"].1, "audio/mpeg");

        // Case 2: Theme without assets/bgm.mp3 still builds normally
        let theme_without_bgm = tempfile::tempdir().unwrap();
        std::fs::write(theme_without_bgm.path().join("index.html"), b"<h1>Cyber</h1>").unwrap();

        let payload_without = create_overlay_zip(&config, theme_without_bgm.path()).unwrap();
        let assets_without = parse_embedded_assets(&payload_without).unwrap();
        assert!(assets_without.files.contains_key("index.html"));
        assert!(!assets_without.files.contains_key("assets/bgm.mp3"));
    }
}
