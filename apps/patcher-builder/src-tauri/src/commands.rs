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

/// Resolves a candidate relative or absolute path by checking:
/// 1. The path directly
/// 2. Relative to `std::env::current_dir()` and up to 4 ancestor directories
/// 3. Relative to `std::env::current_exe()` parent directory and up to 4 ancestor directories
fn resolve_project_path(rel_or_abs: &str) -> Option<PathBuf> {
    let trimmed = rel_or_abs.trim();
    if trimmed.is_empty() {
        return None;
    }

    let direct = PathBuf::from(trimmed);
    if direct.exists() {
        return Some(direct);
    }

    let mut base_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        let mut cur: Option<&Path> = Some(cwd.as_path());
        for _ in 0..5 {
            if let Some(d) = cur {
                base_dirs.push(d.to_path_buf());
                cur = d.parent();
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur: Option<&Path> = exe.parent();
        for _ in 0..5 {
            if let Some(d) = cur {
                if !base_dirs.contains(&d.to_path_buf()) {
                    base_dirs.push(d.to_path_buf());
                }
                cur = d.parent();
            }
        }
    }

    for base in base_dirs {
        let candidate = base.join(trimmed);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

#[tauri::command]
pub fn generate_patcher(req: GeneratePatcherRequest) -> Result<GeneratePatcherResponse, String> {
    log::info!("Generating patcher to {:?}", req.output_exe_path);

    if req.output_exe_path.trim().is_empty() {
        return Err("Output executable path cannot be empty.".to_string());
    }

    // 1. Locate patcher_base.exe template
    let base_template = req
        .template_exe_path
        .as_deref()
        .and_then(resolve_project_path)
        .or_else(|| resolve_project_path("template-dist/patcher_base.exe"))
        .or_else(|| resolve_project_path("target/release/patcher-template.exe"))
        .or_else(|| resolve_project_path("target/debug/patcher-template.exe"))
        .ok_or_else(|| {
            "Could not find patcher_base.exe template. Please ensure template-dist/patcher_base.exe exists."
                .to_string()
        })?;

    let raw_output = PathBuf::from(req.output_exe_path.trim());
    let output_path = if raw_output.is_absolute() {
        raw_output
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(raw_output)
    } else {
        raw_output
    };

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create output directory {:?}: {:#}", parent, e))?;
        }
    }

    // 2. Copy base template to target output file
    fs::copy(&base_template, &output_path).map_err(|e| {
        format!(
            "Failed to copy patcher base template from {:?} to {:?}: {:#}",
            base_template, output_path, e
        )
    })?;

    // 3. (CRITICAL) Update PE Resource (icon) BEFORE appending binary overlay
    let icon_to_apply: Option<PathBuf> = req
        .custom_icon_path
        .as_deref()
        .and_then(resolve_project_path)
        .or_else(|| resolve_project_path("template-dist/default_icon.ico"))
        .or_else(|| resolve_project_path("apps/patcher-builder/src-tauri/icons/icon.ico"));

    if let Some(ref ico_path) = icon_to_apply {
        log::info!("Updating executable icon from {:?}", ico_path);
        if let Err(e) = update_executable_icon(&output_path, ico_path) {
            log::warn!("Warning: Failed to update PE icon ({:#}). Continuing with build.", e);
        }
    }

    // 4. Resolve Web Theme folder
    let theme_folder = req
        .theme_folder_path
        .as_deref()
        .and_then(resolve_project_path)
        .or_else(|| resolve_project_path("themes/modern-cyber"))
        .ok_or_else(|| {
            format!(
                "Could not find theme folder {:?}. Please select a valid theme folder.",
                req.theme_folder_path
            )
        })?;

    // 5. Create in-memory ZIP payload
    let payload = create_overlay_zip(&req.config, &theme_folder)
        .map_err(|e| format!("Failed to create overlay ZIP package: {:#}", e))?;

    // 6. Append Binary Overlay with 48-byte trailer
    append_overlay_to_file(&output_path, &payload)
        .map_err(|e| format!("Failed to append binary overlay: {:#}", e))?;

    let metadata = fs::metadata(&output_path)
        .map_err(|e| format!("Failed to query output metadata: {:#}", e))?;

    let size = metadata.len();
    let display_path = fs::canonicalize(&output_path)
        .unwrap_or_else(|_| output_path.clone())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    Ok(GeneratePatcherResponse {
        success: true,
        output_path: display_path.clone(),
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

#[cfg(target_os = "windows")]
mod win32_dialog {
    use std::ffi::{c_void, OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    #[repr(C)]
    #[allow(non_snake_case)]
    struct OPENFILENAMEW {
        lStructSize: u32,
        hwndOwner: isize,
        hInstance: isize,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: isize,
        lpfnHook: isize,
        lpTemplateName: *const u16,
        pvReserved: *mut c_void,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct BROWSEINFOW {
        hwndOwner: isize,
        pidlRoot: *const c_void,
        pszDisplayName: *mut u16,
        lpszTitle: *const u16,
        ulFlags: u32,
        lpfn: Option<unsafe extern "system" fn(isize, u32, isize, isize) -> i32>,
        lParam: isize,
        iImage: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
        fn GetSaveFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
        fn CommDlgExtendedError() -> u32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHBrowseForFolderW(lpbi: *mut BROWSEINFOW) -> *mut c_void;
        fn SHGetPathFromIDListW(pidl: *const c_void, pszPath: *mut u16) -> i32;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(pv: *mut c_void);
    }

    const OFN_OVERWRITEPROMPT: u32 = 0x00000002;
    const OFN_NOCHANGEDIR: u32 = 0x00000008;
    const OFN_PATHMUSTEXIST: u32 = 0x00000800;
    const OFN_FILEMUSTEXIST: u32 = 0x00001000;
    const OFN_EXPLORER: u32 = 0x00080000;

    const BIF_RETURNONLYFSDIRS: u32 = 0x00000001;
    const BIF_EDITBOX: u32 = 0x00000010;
    const BIF_NEWDIALOGSTYLE: u32 = 0x00000040;
    const BFFM_INITIALIZED: u32 = 1;
    const BFFM_SETSELECTIONW: u32 = 0x0400 + 103;

    unsafe extern "system" fn browse_callback(
        hwnd: isize,
        u_msg: u32,
        _l_param: isize,
        lp_data: isize,
    ) -> i32 {
        if u_msg == BFFM_INITIALIZED && lp_data != 0 {
            SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, lp_data);
        }
        0
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide_buf(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        OsString::from_wide(&buf[..len]).to_string_lossy().to_string()
    }

    pub fn show_folder_dialog(owner_hwnd: isize, initial_dir: Option<&Path>) -> Option<String> {
        unsafe {
            let _ = CoInitializeEx(std::ptr::null_mut(), 0x2); // COINIT_APARTMENTTHREADED
            let hwnd = if owner_hwnd != 0 {
                owner_hwnd
            } else {
                GetForegroundWindow()
            };
            let mut display_buf = vec![0u16; 260];
            let title = to_wide_null("Select Folder");
            let init_wide = initial_dir
                .filter(|p| p.exists())
                .map(|p| to_wide_null(&p.to_string_lossy()));

            let mut bi = BROWSEINFOW {
                hwndOwner: hwnd,
                pidlRoot: std::ptr::null(),
                pszDisplayName: display_buf.as_mut_ptr(),
                lpszTitle: title.as_ptr(),
                ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
                lpfn: if init_wide.is_some() {
                    Some(browse_callback)
                } else {
                    None
                },
                lParam: init_wide
                    .as_ref()
                    .map(|v| v.as_ptr() as isize)
                    .unwrap_or(0),
                iImage: 0,
            };

            let pidl = SHBrowseForFolderW(&mut bi);
            if pidl.is_null() {
                CoUninitialize();
                return None;
            }

            let mut path_buf = vec![0u16; 1024];
            let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
            CoTaskMemFree(pidl);
            CoUninitialize();

            if ok != 0 {
                let s = from_wide_buf(&path_buf);
                if !s.is_empty() {
                    return Some(s);
                }
            }
            None
        }
    }

    pub fn show_file_dialog(
        owner_hwnd: isize,
        is_save: bool,
        overwrite_prompt: bool,
        filter_spec: &[(&str, &str)],
        default_ext: Option<&str>,
        initial_path: Option<&Path>,
    ) -> Result<Option<String>, String> {
        unsafe {
            let _ = CoInitializeEx(std::ptr::null_mut(), 0x2);
            let hwnd = if owner_hwnd != 0 {
                owner_hwnd
            } else {
                GetForegroundWindow()
            };

            let mut filter_wide: Vec<u16> = Vec::new();
            for (label, pattern) in filter_spec {
                filter_wide.extend(OsStr::new(label).encode_wide());
                filter_wide.push(0);
                filter_wide.extend(OsStr::new(pattern).encode_wide());
                filter_wide.push(0);
            }
            filter_wide.push(0);

            let mut file_buf = vec![0u16; 1024];
            let mut init_dir_wide: Option<Vec<u16>> = None;

            if let Some(p) = initial_path {
                if p.is_dir() && p.exists() {
                    init_dir_wide = Some(to_wide_null(&p.to_string_lossy()));
                } else {
                    if let Some(parent) = p.parent() {
                        if parent.exists() {
                            init_dir_wide = Some(to_wide_null(&parent.to_string_lossy()));
                        }
                    }
                    if is_save {
                        if let Some(fname) = p.file_name() {
                            let wide_name: Vec<u16> = fname.encode_wide().collect();
                            if wide_name.len() + 1 < file_buf.len() {
                                file_buf[..wide_name.len()].copy_from_slice(&wide_name);
                            }
                        }
                    }
                }
            }

            let def_ext_wide = default_ext.map(to_wide_null);

            let mut flags = OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_EXPLORER;
            if is_save {
                if overwrite_prompt {
                    flags |= OFN_OVERWRITEPROMPT;
                }
            } else {
                flags |= OFN_FILEMUSTEXIST;
            }

            let mut ofn: OPENFILENAMEW = std::mem::zeroed();
            ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
            ofn.hwndOwner = hwnd;
            ofn.lpstrFilter = filter_wide.as_ptr();
            ofn.nFilterIndex = 1;
            ofn.lpstrFile = file_buf.as_mut_ptr();
            ofn.nMaxFile = file_buf.len() as u32;
            ofn.lpstrInitialDir = init_dir_wide
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null());
            ofn.Flags = flags;
            ofn.lpstrDefExt = def_ext_wide
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null());

            let mut res = if is_save {
                GetSaveFileNameW(&mut ofn)
            } else {
                GetOpenFileNameW(&mut ofn)
            };

            if res == 0 {
                let err_code = CommDlgExtendedError();
                if err_code != 0 {
                    // Fallback: retry with hwndOwner = 0 and clean file_buf
                    file_buf.fill(0);
                    ofn.hwndOwner = 0;
                    ofn.lpstrInitialDir = std::ptr::null();
                    res = if is_save {
                        GetSaveFileNameW(&mut ofn)
                    } else {
                        GetOpenFileNameW(&mut ofn)
                    };
                    if res == 0 {
                        let err_code2 = CommDlgExtendedError();
                        if err_code2 != 0 {
                            CoUninitialize();
                            return Err(format!("Common dialog error code: 0x{:X}", err_code2));
                        }
                    }
                }
            }

            CoUninitialize();

            if res != 0 {
                let s = from_wide_buf(&file_buf);
                if !s.is_empty() {
                    return Ok(Some(s));
                }
            }
            Ok(None)
        }
    }

    pub fn resolve_initial_path(current_path: Option<&str>) -> Option<PathBuf> {
        let s = current_path?.trim();
        if s.is_empty() {
            return std::env::current_dir().ok();
        }
        if let Some(resolved) = super::resolve_project_path(s) {
            return Some(resolved);
        }
        let pb = PathBuf::from(s);
        if pb.is_absolute() {
            return Some(pb);
        }
        std::env::current_dir().ok().map(|cwd| cwd.join(pb))
    }
}

#[tauri::command]
pub async fn pick_path(
    window: tauri::WebviewWindow,
    mode: String,
    current_path: Option<String>,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let owner_hwnd = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
        let handle = std::thread::spawn(move || -> Result<Option<String>, String> {
            let init_path = win32_dialog::resolve_initial_path(current_path.as_deref());
            let init_ref = init_path.as_deref();
            let picked = match mode.as_str() {
                "folder" => Ok(win32_dialog::show_folder_dialog(owner_hwnd, init_ref)),
                "file_ico" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    false,
                    false,
                    &[("Icon Files (*.ico)", "*.ico"), ("All Files (*.*)", "*.*")],
                    Some("ico"),
                    init_ref,
                ),
                "file_exe" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    false,
                    false,
                    &[
                        ("Executable Files (*.exe)", "*.exe"),
                        ("All Files (*.*)", "*.*"),
                    ],
                    Some("exe"),
                    init_ref,
                ),
                "file_any" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    false,
                    false,
                    &[("All Files (*.*)", "*.*")],
                    None,
                    init_ref,
                ),
                "save_thor" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    true,
                    true,
                    &[
                        ("THOR Patch Archive (*.thor)", "*.thor"),
                        ("All Files (*.*)", "*.*"),
                    ],
                    Some("thor"),
                    init_ref,
                ),
                "save_exe" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    true,
                    true,
                    &[
                        ("Standalone Patcher (*.exe)", "*.exe"),
                        ("All Files (*.*)", "*.*"),
                    ],
                    Some("exe"),
                    init_ref,
                ),
                "file_plist" => win32_dialog::show_file_dialog(
                    owner_hwnd,
                    true,
                    false,
                    &[
                        ("Patch List (plist.txt)", "*.txt"),
                        ("All Files (*.*)", "*.*"),
                    ],
                    Some("txt"),
                    init_ref,
                ),
                _ => return Err(format!("Unsupported picker mode: {}", mode)),
            }?;
            Ok(picked)
        });

        handle
            .join()
            .map_err(|_| "File dialog thread panicked".to_string())?
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, mode, current_path);
        Ok(None)
    }
}

#[tauri::command]
pub async fn import_delete_list_file(window: tauri::WebviewWindow) -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let owner_hwnd = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
        let selected_opt = std::thread::spawn(move || {
            let init_path = std::env::current_dir().ok();
            win32_dialog::show_file_dialog(
                owner_hwnd,
                false,
                false,
                &[("Text List (*.txt)", "*.txt"), ("All Files (*.*)", "*.*")],
                Some("txt"),
                init_path.as_deref(),
            )
        })
        .join()
        .map_err(|_| "File dialog thread panicked".to_string())??;

        let Some(selected) = selected_opt else {
            return Ok(Vec::new());
        };
        let content = fs::read_to_string(&selected)
            .map_err(|e| format!("Failed to read delete list file: {:#}", e))?;
        Ok(parse_delete_list(&content))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(Vec::new())
    }
}
