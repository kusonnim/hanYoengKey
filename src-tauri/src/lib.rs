mod converter;
mod core;
mod hook;
mod replace;
mod selection;
mod settings;
mod tray;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(settings::handle_window_event)
        .run(tauri::generate_context!())
        .expect("failed to run HanYeongKey");
}
