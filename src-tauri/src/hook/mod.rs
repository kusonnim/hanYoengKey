//! Global keyboard event detection.
//!
//! This module translates platform input into application-level keyboard
//! events. It never interprets those events or invokes business services.

mod event;

#[cfg(windows)]
mod windows;

pub(crate) use event::KeyboardEvent;
#[cfg(windows)]
pub(crate) use windows::{complete_event, HookError, KeyboardHook};
