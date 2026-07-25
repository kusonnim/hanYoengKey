use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use crate::{
    hook::{complete_event, KeyboardEvent},
    settings::{HotkeyMode, Settings, SettingsRuntime},
};

use super::coordinator::ApplicationConversionCoordinator;

pub(super) struct EventDispatcher {
    worker: Option<JoinHandle<()>>,
}

impl EventDispatcher {
    pub(super) fn start(
        settings_runtime: SettingsRuntime,
        settings_updates: mpsc::Receiver<Settings>,
    ) -> std::io::Result<(Self, Sender<KeyboardEvent>)> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("event-dispatcher".into())
            .spawn(move || {
                let coordinator = ApplicationConversionCoordinator::application();
                let mut settings = settings_runtime.current();

                while let Ok(event) = receiver.recv() {
                    for update in settings_updates.try_iter() {
                        settings = update;
                    }
                    match event {
                        KeyboardEvent::HangulKeyPressed => {
                            if !conversion_is_enabled(&settings) {
                                complete_event(false);
                                continue;
                            }
                            let outcome = coordinator.process(&settings);
                            complete_event(outcome.handled());
                        }
                    }
                }
            })?;

        Ok((
            Self {
                worker: Some(worker),
            },
            sender,
        ))
    }
}

fn conversion_is_enabled(settings: &Settings) -> bool {
    settings.enable_conversion && matches!(settings.hotkey_mode, HotkeyMode::HangulEnglishKey)
}

impl Drop for EventDispatcher {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_setting_bypasses_conversion() {
        let settings = Settings {
            enable_conversion: false,
            ..Settings::default()
        };
        assert!(!conversion_is_enabled(&settings));
    }

    #[test]
    fn supported_hotkey_mode_is_enabled() {
        assert!(conversion_is_enabled(&Settings::default()));
    }
}
