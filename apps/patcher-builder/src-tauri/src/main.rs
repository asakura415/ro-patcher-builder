#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::{
    create_thor_patch, generate_patcher, get_default_config, import_delete_list_file,
    inspect_theme_folder, open_external_url, pick_path, scan_game_directory,
};

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_default_config,
            scan_game_directory,
            generate_patcher,
            create_thor_patch,
            open_external_url,
            pick_path,
            import_delete_list_file,
            inspect_theme_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
