use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use patcher_core::config::PatcherConfig;
use patcher_core::overlay::{append_overlay_to_file, create_overlay_zip};
use patcher_core::pe::update_executable_icon;
use patch_maker::{
    add_patch_to_plist, build_thor_patch, collect_entries_from_directory, parse_delete_list,
    FilenameEncoding, PatchEntry, ThorBuildOptions,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratePatcherRequest {
    pub config: PatcherConfig,
    pub output_exe_path: String,
    pub template_exe_path: Option<String>,
    pub custom_icon_path: Option<String>,
    pub theme_folder_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratePatcherResponse {
    pub success: bool,
    pub output_path: String,
    pub file_size_bytes: u64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateThorRequest {
    #[serde(default = "default_input_mode")]
    pub input_mode: String,
    #[serde(default)]
    pub input_dir: String,
    #[serde(default)]
    pub single_file_target_path: Option<String>,
    pub output_thor_path: String,
    pub use_grf_merging: bool,
    pub target_grf: Option<String>,
    pub include_checksums: bool,
    #[serde(default = "default_compression_level")]
    pub compression_level: u32,
    #[serde(default)]
    pub filename_encoding: Option<String>,
    #[serde(default)]
    pub delete_files: Vec<String>,
    pub update_plist_path: Option<String>,
    pub forced_patch_index: Option<u32>,
}

fn default_input_mode() -> String {
    "directory".to_string()
}

fn default_compression_level() -> u32 {
    9
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateThorResponse {
    pub success: bool,
    pub files_packaged: usize,
    pub output_thor_path: String,
    pub plist_index: Option<u32>,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameDirScanResult {
    pub found_clients: Vec<String>,
    pub found_settings: Vec<String>,
    pub found_grfs: Vec<String>,
    pub found_icons: Vec<String>,
}

#[tauri::command]
pub fn get_default_config() -> PatcherConfig {
    let mut config = PatcherConfig::default();
    config.server.patch_servers.push(patcher_core::config::PatchServerInfo {
        name: "Main Server".to_string(),
        plist_url: "http://patch.pronterabox.com/patch/plist.txt".to_string(),
        patch_url: "http://patch.pronterabox.com/patch/".to_string(),
    });
    config.server.preferred_server = Some("Main Server".to_string());
    config.client.executable = "_PRONTERA BOX.exe".to_string();
    config.client.default_grf_name = "_prontera_box.grf".to_string();
    config.client.setup_executable = Some("_PRONTERA BOX - Settings.exe".to_string());
    config.window.title = "PRONTERA BOX - Patcher".to_string();
    config.window.width = 800;
    config.window.height = 500;
    config
}

#[tauri::command]
pub fn scan_game_directory(dir_path: String) -> Result<GameDirScanResult, String> {
    let path = Path::new(&dir_path);
    if !path.exists() || !path.is_dir() {
        return Err("Directory does not exist or is not a folder".to_string());
    }

    let mut found_clients = Vec::new();
    let mut found_settings = Vec::new();
    let mut found_grfs = Vec::new();
    let mut found_icons = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let filename = entry.file_name().to_string_lossy().to_string();
            let lower = filename.to_lowercase();

            if lower.ends_with(".exe") {
                if lower.contains("setting") || lower.contains("setup") {
                    found_settings.push(filename);
                } else if !lower.contains("patcher") && !lower.contains("unins") {
                    found_clients.push(filename);
                }
            } else if lower.ends_with(".grf") {
                found_grfs.push(filename);
            } else if lower.ends_with(".ico") {
                found_icons.push(filename);
            }
        }
    }

    Ok(GameDirScanResult {
        found_clients,
        found_settings,
        found_grfs,
        found_icons,
    })
}

#[tauri::command]
pub fn generate_patcher(req: GeneratePatcherRequest) -> Result<GeneratePatcherResponse, String> {
    log::info!("Generating patcher to {:?}", req.output_exe_path);

    // 1. Locate patcher_base.exe template
    let base_template = match req.template_exe_path {
        Some(ref p) if Path::new(p).exists() => PathBuf::from(p),
        _ => {
            let possible_paths = [
                PathBuf::from("template-dist/patcher_base.exe"),
                PathBuf::from("../template-dist/patcher_base.exe"),
                PathBuf::from("../../template-dist/patcher_base.exe"),
                PathBuf::from("target/release/patcher-template.exe"),
                PathBuf::from("../target/release/patcher-template.exe"),
                PathBuf::from("../../target/release/patcher-template.exe"),
            ];

            possible_paths
                .iter()
                .find(|p| p.exists())
                .cloned()
                .ok_or_else(|| {
                    "Could not find patcher_base.exe template. Please compile patcher-template first."
                        .to_string()
                })?
        }
    };

    let output_path = PathBuf::from(&req.output_exe_path);
    if let Some(parent) = output_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // 2. Copy base template to target output file
    fs::copy(&base_template, &output_path)
        .map_err(|e| format!("Failed to copy patcher base template: {:#}", e))?;

    // 3. (CRITICAL) Update PE Resource (icon) BEFORE appending binary overlay
    let icon_to_apply: Option<PathBuf> = match req.custom_icon_path {
        Some(ref p) if !p.trim().is_empty() && Path::new(p).exists() => Some(PathBuf::from(p)),
        _ => {
            let default_icons = [
                PathBuf::from("template-dist/default_icon.ico"),
                PathBuf::from("../template-dist/default_icon.ico"),
                PathBuf::from("../../template-dist/default_icon.ico"),
                PathBuf::from("apps/patcher-builder/src-tauri/icons/icon.ico"),
            ];
            default_icons.into_iter().find(|p| p.exists())
        }
    };

    if let Some(ref ico_path) = icon_to_apply {
        log::info!("Updating executable icon from {:?}", ico_path);
        if let Err(e) = update_executable_icon(&output_path, ico_path) {
            log::warn!("Warning: Failed to update PE icon ({:#}). Continuing with build.", e);
        }
    }

    // 4. Resolve Web Theme folder
    let theme_folder = match req.theme_folder_path {
        Some(ref p) if Path::new(p).exists() => PathBuf::from(p),
        Some(ref p) if Path::new(&format!("../{}", p)).exists() => PathBuf::from(&format!("../{}", p)),
        Some(ref p) if Path::new(&format!("../../{}", p)).exists() => PathBuf::from(&format!("../../{}", p)),
        _ => {
            let possible_themes = [
                PathBuf::from("themes/modern-cyber"),
                PathBuf::from("../themes/modern-cyber"),
                PathBuf::from("../../themes/modern-cyber"),
            ];
            possible_themes
                .iter()
                .find(|p| p.exists())
                .cloned()
                .unwrap_or_else(|| PathBuf::from("themes/modern-cyber"))
        }
    };

    // 5. Create in-memory ZIP payload
    let payload = create_overlay_zip(&req.config, &theme_folder)
        .map_err(|e| format!("Failed to create overlay ZIP package: {:#}", e))?;

    // 6. Append Binary Overlay with 48-byte trailer
    append_overlay_to_file(&output_path, &payload)
        .map_err(|e| format!("Failed to append binary overlay: {:#}", e))?;

    let metadata = fs::metadata(&output_path)
        .map_err(|e| format!("Failed to query output metadata: {:#}", e))?;

    let size = metadata.len();
    Ok(GeneratePatcherResponse {
        success: true,
        output_path: output_path.to_string_lossy().to_string(),
        file_size_bytes: size,
        message: format!(
            "Successfully generated standalone patcher! Output: {} ({:.2} MB)",
            output_path.file_name().unwrap_or_default().to_string_lossy(),
            size as f64 / (1024.0 * 1024.0)
        ),
    })
}

#[tauri::command]
pub fn create_thor_patch(req: CreateThorRequest) -> Result<CreateThorResponse, String> {
    log::info!(
        "Building THOR patch (mode: {}, input: {:?})",
        req.input_mode,
        req.input_dir
    );

    let output_thor = Path::new(&req.output_thor_path);
    let is_exe_mode = req.input_mode == "patcher_exe";
    let use_grf_merging = if is_exe_mode { false } else { req.use_grf_merging };

    let encoding = match req.filename_encoding.as_deref() {
        Some("ascii") => FilenameEncoding::Ascii,
        _ => FilenameEncoding::Unicode,
    };

    let options = ThorBuildOptions {
        use_grf_merging,
        target_grf_name: if use_grf_merging {
            req.target_grf.filter(|s| !s.trim().is_empty())
        } else {
            None
        },
        include_checksums: req.include_checksums,
        compression_level: req.compression_level.clamp(0, 9),
        filename_encoding: encoding,
    };

    let mut entries: Vec<PatchEntry> = Vec::new();

    match req.input_mode.as_str() {
        "directory" => {
            if !req.input_dir.trim().is_empty() {
                let input_dir = Path::new(&req.input_dir);
                if !input_dir.exists() || !input_dir.is_dir() {
                    return Err("Input directory does not exist or is not a folder".to_string());
                }
                let dir_entries = collect_entries_from_directory(input_dir)
                    .map_err(|e| format!("Failed scanning directory: {:#}", e))?;
                entries.extend(dir_entries);
            } else if req.delete_files.is_empty() {
                return Err("Please specify an input folder or add files to the Delete List.".to_string());
            }
        }
        "single_file" | "patcher_exe" => {
            if !req.input_dir.trim().is_empty() {
                let src_file = Path::new(&req.input_dir);
                if !src_file.exists() || !src_file.is_file() {
                    return Err("Selected input file does not exist.".to_string());
                }
                let default_name = src_file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let target_rel = if is_exe_mode {
                    default_name
                } else {
                    req.single_file_target_path
                        .as_ref()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(default_name)
                };
                entries.push(PatchEntry {
                    target_relative_path: target_rel,
                    is_removed: false,
                    source_path: Some(src_file.to_path_buf()),
                });
            } else if req.delete_files.is_empty() {
                return Err("Please select a file to package.".to_string());
            }
        }
        "delete_only" => {
            if req.delete_files.is_empty() {
                return Err("Delete list is empty. Please add at least one file path to delete.".to_string());
            }
        }
        _ => return Err(format!("Unsupported input mode: {}", req.input_mode)),
    }

    for del_path in &req.delete_files {
        let trimmed = del_path.trim();
        if !trimmed.is_empty() {
            entries.push(PatchEntry {
                target_relative_path: trimmed.to_string(),
                is_removed: true,
                source_path: None,
            });
        }
    }

    if entries.is_empty() {
        return Err("No files or deletion entries found to package into THOR archive.".to_string());
    }

    let files_packaged = entries.len();
    build_thor_patch(output_thor, &options, &entries)
        .map_err(|e| format!("Failed to build THOR patch: {:#}", e))?;

    let mut plist_index = None;
    if let Some(plist_path_str) = req.update_plist_path {
        let plist_path = Path::new(&plist_path_str);
        let thor_name = output_thor
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("patch.thor");

        match add_patch_to_plist(plist_path, thor_name, req.forced_patch_index) {
            Ok(idx) => plist_index = Some(idx),
            Err(e) => return Err(format!("Failed to update plist.txt: {:#}", e)),
        }
    }

    Ok(CreateThorResponse {
        success: true,
        files_packaged,
        output_thor_path: output_thor.to_string_lossy().to_string(),
        plist_index,
        message: format!(
            "Successfully created THOR patch containing {} entries!",
            files_packaged
        ),
    })
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only http:// and https:// URLs are allowed.".to_string());
    }

    let safe_url = parsed.as_str();
    log::info!("Opening external URL: {}", safe_url);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", safe_url])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to launch browser: {:#}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(safe_url)
            .spawn()
            .map_err(|e| format!("Failed to launch browser: {:#}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pick_path(mode: String) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = match mode.as_str() {
            "folder" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.SelectedPath }"
            }
            "file_ico" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'Icon Files (*.ico)|*.ico|All Files (*.*)|*.*'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            "file_exe" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'Executable Files (*.exe)|*.exe|All Files (*.*)|*.*'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            "file_any" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'All Files (*.*)|*.*'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            "save_thor" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.SaveFileDialog; $d.Filter = 'THOR Patch Archive (*.thor)|*.thor'; $d.DefaultExt = 'thor'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            "save_exe" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.SaveFileDialog; $d.Filter = 'Standalone Patcher (*.exe)|*.exe'; $d.DefaultExt = 'exe'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            "file_plist" => {
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.SaveFileDialog; $d.Filter = 'Patch List (plist.txt)|*.txt|All Files (*.*)|*.*'; $d.OverwritePrompt = $false; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }"
            }
            _ => return Err("Unsupported picker mode".to_string()),
        };

        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to open file dialog: {:#}", e))?;

        let selected = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(selected))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = mode;
        Ok(None)
    }
}

#[tauri::command]
pub async fn import_delete_list_file() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'Text List (*.txt)|*.txt|All Files (*.*)|*.*'; if ($d.ShowDialog() -eq 'OK') { Write-Output $d.FileName }";
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to open file dialog: {:#}", e))?;

        let selected = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&selected)
            .map_err(|e| format!("Failed to read delete list file: {:#}", e))?;
        Ok(parse_delete_list(&content))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}
