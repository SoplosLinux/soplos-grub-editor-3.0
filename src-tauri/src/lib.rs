mod commands;
mod system_theme;

pub use commands::privileged::run_ops_file;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin registered, same as the other Soplos
        // Tauri apps: it needs to intercept and exit a second launch before
        // anything else runs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            system_theme::get_system_theme,
            commands::environment::get_environment_info,
            commands::jobs::poll_job,
            commands::privileged::run_privileged_ops,
            commands::grub_read::grub_present,
            commands::grub_read::read_grub_config,
            commands::grub_read::get_menu_entries,
            commands::grub_read::list_themes,
            commands::grub_read::list_fonts,
            commands::grub_read::read_custom_colors,
            commands::grub_read::read_image_preview,
            commands::grub_read::read_theme_preview,
            commands::grub_read::save_grub_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Soplos GRUB Editor");
}
