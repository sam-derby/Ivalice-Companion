fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    let result = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_save_selection",
            "set_save_selection",
            "load_reader",
            "preview_draft",
            "preview_base_stat",
            "preview_character",
            "preview_job_progress",
            "save_transaction",
            "restore_last_backup",
            "slot_operation",
            "export_save_slot",
            "inspect_save_file",
        ]),
    ));
    if let Err(error) = result {
        eprintln!("failed to configure the Tauri build: {error}");
        std::process::exit(1);
    }
}
