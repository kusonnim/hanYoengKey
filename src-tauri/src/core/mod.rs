//! Application lifecycle coordination.

mod dispatcher;

use crate::hook::{HookError, KeyboardHook};
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
    pub(crate) fn start() -> Result<Self, CoreError> {
        let (dispatcher, event_sender) =
            EventDispatcher::start().map_err(CoreError::DispatcherThread)?;
        let keyboard_hook = KeyboardHook::install(event_sender)?;

        Ok(Self {
            _keyboard_hook: keyboard_hook,
            _dispatcher: dispatcher,
        })
    }
}
