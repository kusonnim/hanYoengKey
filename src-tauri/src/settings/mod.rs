use tauri::{Window, WindowEvent};

pub(crate) const WINDOW_LABEL: &str = "settings";

pub(crate) fn handle_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != WINDOW_LABEL {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}
