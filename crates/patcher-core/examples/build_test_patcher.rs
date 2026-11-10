use std::path::Path;
use patcher_core::config::{PatcherConfig, PatchServerInfo};
use patcher_core::overlay::{append_overlay_to_file, create_overlay_zip};
use patcher_core::pe::update_executable_icon;

fn main() -> anyhow::Result<()> {
    let base_template = Path::new("template-dist/patcher_base.exe");
    let output_exe = Path::new("test_env/client/_PRONTERA BOX - Patcher.exe");
    let icon_path = Path::new("apps/patcher-builder/src-tauri/icons/icon.ico");
    let theme_dir = Path::new("themes/modern-cyber");

    if let Some(parent) = output_exe.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("Copying base template {:?} to {:?}", base_template, output_exe);
    std::fs::copy(base_template, output_exe)?;

    if icon_path.exists() {
        println!("Updating PE icon from {:?}", icon_path);
        if let Err(e) = update_executable_icon(output_exe, icon_path) {
            eprintln!("Warning: could not update icon: {}", e);
        }
    }

    let mut config = PatcherConfig::default();
    config.server.patch_servers.push(PatchServerInfo {
        name: "Localhost Test Server".to_string(),
        plist_url: "http://127.0.0.1:8080/plist.txt".to_string(),
        patch_url: "http://127.0.0.1:8080/".to_string(),
    });
    config.server.preferred_server = Some("Localhost Test Server".to_string());
    config.server.notice_url = Some("http://127.0.0.1:8080/notice.html".to_string());
    config.client.executable = "_PRONTERA BOX.exe".to_string();
    config.client.arguments = vec!["-1rag1".to_string()];
    config.client.setup_executable = Some("_PRONTERA BOX - Settings.exe".to_string());
    config.client.default_grf_name = "_prontera_box.grf".to_string();
    config.window.title = "PRONTERA BOX - Patcher".to_string();
    config.window.width = 820;
    config.window.height = 540;
    config.window.transparent = true;

    println!("Creating overlay ZIP from {:?}", theme_dir);
    let payload = create_overlay_zip(&config, theme_dir)?;

    println!("Appending overlay ({} bytes) to {:?}", payload.len(), output_exe);
    append_overlay_to_file(output_exe, &payload)?;

    let size = std::fs::metadata(output_exe)?.len();
    println!("SUCCESS! Output standalone patcher created at {:?} ({} bytes / {:.2} MB)", output_exe, size, size as f64 / (1024.0 * 1024.0));
    Ok(())
}
