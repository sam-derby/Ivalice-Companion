#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ipc;

use tauri::Manager;

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let resource_root = app.path().resource_dir()?;
            app.manage(ipc::DesktopState::for_current_user(resource_root));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_save_selection,
            ipc::set_save_selection,
            ipc::load_reader,
            ipc::preview_character,
            ipc::preview_job_progress,
            ipc::save_transaction,
            ipc::restore_last_backup
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Could not launch Ivalice Companion: {error}");
        std::process::exit(1);
    }
}
