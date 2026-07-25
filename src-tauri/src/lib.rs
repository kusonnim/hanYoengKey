#[cfg(windows)]
mod clipboard;
pub mod converter;
mod core;
mod hook;
mod replace;
mod selection;
mod settings;
mod tray;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let core = core::ApplicationCore::start()?;
            app.manage(core);
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(settings::handle_window_event)
        .run(tauri::generate_context!())
        .expect("failed to run HanYeongKey");
}
