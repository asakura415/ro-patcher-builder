use std::fs::File;
use std::path::Path;
use patcher_core::config::{PatcherConfig, PatchServerInfo};
use patcher_core::overlay::{
    append_overlay_to_file, create_overlay_zip, parse_embedded_assets, read_overlay_from_reader,
};
use patcher_core::pe::update_executable_icon;
use tempfile::tempdir;

#[test]
fn test_generate_standalone_patcher_end_to_end() {
    let temp = tempdir().unwrap();
    let output_exe = temp.path().join("_PRONTERA_BOX_Patcher.exe");

    // 1. Locate base template
    let possible_base = [
        Path::new("template-dist/patcher_base.exe"),
        Path::new("../../template-dist/patcher_base.exe"),
        Path::new("D:/Antigravity/patcher-builder/template-dist/patcher_base.exe"),
    ];
    let base_template = possible_base.iter().find(|p| p.exists()).expect("patcher_base.exe not found");

    // Copy template
    std::fs::copy(base_template, &output_exe).unwrap();

    // 2. Update PE icon using our own project icon
    let possible_icos = [
        Path::new("template-dist/default_icon.ico"),
        Path::new("../../template-dist/default_icon.ico"),
        Path::new("D:/Antigravity/patcher-builder/template-dist/default_icon.ico"),
    ];
    if let Some(custom_ico) = possible_icos.iter().find(|p| p.exists()) {
        let update_res = update_executable_icon(&output_exe, custom_ico);
        assert!(update_res.is_ok(), "Failed to update PE icon: {:?}", update_res.err());
    }

    // 3. Configure for PRONTERA BOX
    let mut config = PatcherConfig::default();
    config.server.patch_servers.push(PatchServerInfo {
        name: "PRONTERA BOX Main".to_string(),
        plist_url: "http://patch.pronterabox.com/patch/plist.txt".to_string(),
        patch_url: "http://patch.pronterabox.com/patch/".to_string(),
    });
    config.client.executable = "_PRONTERA BOX.exe".to_string();
    config.client.arguments = vec!["-1rag1".to_string()];
    config.client.setup_executable = Some("_PRONTERA BOX - Settings.exe".to_string());
    config.client.default_grf_name = "_prontera_box.grf".to_string();
    config.window.title = "PRONTERA BOX - Patcher".to_string();

    // 4. Package themes/modern-cyber into ZIP overlay
    let possible_themes = [
        Path::new("themes/modern-cyber"),
        Path::new("../../themes/modern-cyber"),
        Path::new("D:/Antigravity/patcher-builder/themes/modern-cyber"),
    ];
    let theme_dir = possible_themes.iter().find(|p| p.exists()).expect("themes/modern-cyber not found");
    let payload = create_overlay_zip(&config, theme_dir).unwrap();
    assert!(!payload.is_empty());

    // 5. Append binary overlay
    append_overlay_to_file(&output_exe, &payload).unwrap();

    // 6. Verification: Read overlay back from generated executable
    let mut file = File::open(&output_exe).unwrap();
    let read_payload = read_overlay_from_reader(&mut file).unwrap();
    assert!(read_payload.is_some(), "Binary overlay trailer not found");

    let assets = parse_embedded_assets(&read_payload.unwrap()).unwrap();
    assert_eq!(assets.config.client.executable, "_PRONTERA BOX.exe");
    assert_eq!(assets.config.client.default_grf_name, "_prontera_box.grf");
    assert_eq!(assets.config.window.title, "PRONTERA BOX - Patcher");

    // Verify web assets are embedded in memory
    assert!(assets.files.contains_key("index.html"));
    assert!(assets.files.contains_key("style.css"));
    assert!(assets.files.contains_key("script.js"));

    let index_html_str = String::from_utf8(assets.files["index.html"].0.clone()).unwrap();
    assert!(index_html_str.contains("PRONTERA BOX"));

    println!(
        "End-to-end Patcher generation verified successfully! File size: {} bytes",
        std::fs::metadata(&output_exe).unwrap().len()
    );
}
