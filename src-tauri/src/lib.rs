#[cfg(windows)]
mod clipboard;
pub mod converter;
mod core;
mod hook;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod input_language;
mod replace;
mod selection;
mod settings;
#[cfg(windows)]
mod target;
mod tray;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::SettingsStore::load(settings_path)?;
            let core = core::ApplicationCore::start(settings.runtime(), settings.subscribe())?;
            app.manage(settings);
            app.manage(core);
            tray::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::load_settings,
            settings::save_settings,
            settings::reset_defaults
        ])
        .on_window_event(settings::handle_window_event)
        .run(tauri::generate_context!())
        .expect("failed to run HanYeongKey");
}
