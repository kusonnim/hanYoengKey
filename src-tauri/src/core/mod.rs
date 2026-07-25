//! Application lifecycle coordination.

mod coordinator;
mod direction;
mod dispatcher;

use crate::{
    hook::{HookError, KeyboardHook},
    settings::{Settings, SettingsRuntime},
};
use dispatcher::EventDispatcher;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CoreError {
    #[error("failed to start the event dispatcher: {0}")]
    DispatcherThread(#[source] std::io::Error),
    #[error(transparent)]
    KeyboardHook(#[from] HookError),
}

pub(crate) struct ApplicationCore {
    // Field order is intentional: the hook must drop its event sender before
    // the dispatcher joins its receiving thread.
    _keyboard_hook: KeyboardHook,
    _dispatcher: EventDispatcher,
}

impl ApplicationCore {
    pub(crate) fn start(
        settings: SettingsRuntime,
        settings_updates: std::sync::mpsc::Receiver<Settings>,
    ) -> Result<Self, CoreError> {
        eprintln!("[lifecycle] application-starting");
        let (dispatcher, event_sender) = EventDispatcher::start(settings, settings_updates)
            .map_err(CoreError::DispatcherThread)?;
        let keyboard_hook = KeyboardHook::install(event_sender)?;

        eprintln!("[lifecycle] application-started");
        Ok(Self {
            _keyboard_hook: keyboard_hook,
            _dispatcher: dispatcher,
        })
    }
}

impl Drop for ApplicationCore {
    fn drop(&mut self) {
        eprintln!("[lifecycle] application-stopping");
    }
}
