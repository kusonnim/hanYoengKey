use std::{
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    System::DataExchange::GetClipboardSequenceNumber, UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
};

use crate::{
    clipboard::{self, ClipboardError},
    input::send_control_shortcut,
};

use super::{
    provider::SelectionProvider,
    result::{SelectionError, SelectionResult, SelectionSnapshot},
};

const COPY_TIMEOUT: Duration = Duration::from_millis(300);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(10);
const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

pub(super) struct ClipboardProvider;

impl SelectionProvider for ClipboardProvider {
    fn get_selected_text(&self) -> SelectionResult {
        match clipboard_selection() {
            Ok(Some(text)) if !text.is_empty() => SelectionSnapshot::capture(text)
                .map(SelectionResult::Success)
                .unwrap_or(SelectionResult::TargetChanged),
            Ok(_) => SelectionResult::NoSelection,
            Err(ClipboardError::Restore(error)) => {
                SelectionResult::Failure(SelectionError::ClipboardRestore(error))
            }
            Err(error) => SelectionResult::Failure(SelectionError::Clipboard(error.to_string())),
        }
    }
}

fn clipboard_selection() -> Result<Option<String>, ClipboardError> {
    clipboard::transaction(|snapshot| {
        let sequence_before = unsafe { GetClipboardSequenceNumber() };
        send_copy_shortcut()?;
        if wait_for_clipboard_change(sequence_before) {
            snapshot.mark_current_as_owned();
            clipboard::read_unicode_text()
        } else {
            Ok(None)
        }
    })
}

fn send_copy_shortcut() -> Result<(), ClipboardError> {
    send_control_shortcut(VK_C).map_err(|_| ClipboardError::ShortcutUnavailable)
}

fn wait_for_clipboard_change(previous: u32) -> bool {
    let deadline = Instant::now() + COPY_TIMEOUT;
    while Instant::now() < deadline {
        if unsafe { GetClipboardSequenceNumber() } != previous {
            return true;
        }
        thread::sleep(COPY_POLL_INTERVAL);
    }
    false
}
