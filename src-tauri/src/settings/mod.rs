//! Typed application configuration and its frontend boundary.

mod model;
mod startup;
mod store;

pub(crate) use model::{HotkeyMode, ProviderPreference, Settings};
pub(crate) use store::{SettingsRuntime, SettingsStore};

use tauri::{State, Window, WindowEvent};

pub(crate) const WINDOW_LABEL: &str = "settings";

#[tauri::command]
pub(crate) fn load_settings(state: State<'_, SettingsStore>) -> Settings {
    state.current()
}

#[tauri::command]
pub(crate) fn save_settings(
    settings: Settings,
    state: State<'_, SettingsStore>,
) -> Result<Settings, String> {
    state.update(settings).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn reset_defaults(state: State<'_, SettingsStore>) -> Result<Settings, String> {
    state.reset_defaults().map_err(|error| error.to_string())
}

pub(crate) fn handle_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != WINDOW_LABEL {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}
