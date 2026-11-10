use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use gruf::grf::{GrfArchive, GrfArchiveBuilder};
use gruf::thor::ThorArchiveBuilder;
use patcher_core::config::PatcherConfig;
use patcher_core::patching::apply_single_patch;
use tempfile::tempdir;

#[test]
fn test_read_real_game_grf_if_exists() {
    let grf_path = Path::new("D:\\ro\\PRONTERA BOX\\_block_prontera.grf");
    if grf_path.exists() {
        let grf = GrfArchive::open(grf_path).expect("Failed to open real GRF _block_prontera.grf");
        println!("Real GRF opened successfully! File count: {}", grf.file_count());
        assert!(grf.file_count() > 0);
    }
}

#[test]
fn test_end_to_end_grf_patching() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let grf_path = game_dir.join("test.grf");

    // 1. Create initial GRF with initial content
    {
        let file = File::create(&grf_path).unwrap();
        let mut builder = GrfArchiveBuilder::create(file, 2, 0).unwrap();
        builder
            .add_file("data\\test.txt".to_string(), Cursor::new(b"Initial Content"))
            .unwrap();
    }

    // Verify initial GRF
    {
        let mut grf = GrfArchive::open(&grf_path).unwrap();
        let content = grf.read_file_content("data\\test.txt").unwrap();
        assert_eq!(content, b"Initial Content");
    }

    // 2. Create a THOR patch that updates data\test.txt and adds data\new.txt
    let thor_path = game_dir.join("001_update.thor");
    {
        let file = File::create(&thor_path).unwrap();
        let mut builder = ThorArchiveBuilder::new(
            file,
            true,
            Some("test.grf".to_string()),
            false,
        )
        .unwrap();
        builder
            .append_file_update("data\\test.txt".to_string(), Cursor::new(b"Antigravity Patched!"))
            .unwrap();
        builder
            .append_file_update("data\\new.txt".to_string(), Cursor::new(b"Brand New File"))
            .unwrap();
    }

    // 3. Apply the patch
    let mut config = PatcherConfig::default();
    config.client.default_grf_name = "test.grf".to_string();
    config.patching.in_place = true;

    let self_updated = apply_single_patch(&thor_path, &config, game_dir).unwrap();
    assert!(!self_updated);

    // 4. Verify GRF contents after patch
    {
        let mut grf = GrfArchive::open(&grf_path).unwrap();
        let buf1 = grf.read_file_content("data\\test.txt").unwrap();
        assert_eq!(buf1, b"Antigravity Patched!");

        let buf2 = grf.read_file_content("data\\new.txt").unwrap();
        assert_eq!(buf2, b"Brand New File");
    }
}

#[test]
fn test_end_to_end_disk_patching() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();

    // Create a THOR patch for disk root
    let thor_path = game_dir.join("002_disk.thor");
    {
        let file = File::create(&thor_path).unwrap();
        let mut builder = ThorArchiveBuilder::new(file, false, None, false).unwrap();
        builder
            .append_file_update("System\\itemInfo.lub".to_string(), Cursor::new(b"iteminfo test content"))
            .unwrap();
    }

    let config = PatcherConfig::default();
    apply_single_patch(&thor_path, &config, game_dir).unwrap();

    let target_file = game_dir.join("System").join("itemInfo.lub");
    assert!(target_file.exists());
    let mut read_buf = Vec::new();
    File::open(target_file).unwrap().read_to_end(&mut read_buf).unwrap();
    assert_eq!(read_buf, b"iteminfo test content");
}
