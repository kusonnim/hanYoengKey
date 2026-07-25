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
    selection::SelectionSnapshot,
};

use super::{
    provider::ReplaceProvider,
    result::{ReplaceError, ReplaceResult},
};

const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);
const COPY_TIMEOUT: Duration = Duration::from_millis(300);
const PASTE_SETTLE_TIME: Duration = Duration::from_millis(50);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) struct ClipboardProvider;

impl ReplaceProvider for ClipboardProvider {
    fn replace_selected_text(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
    ) -> ReplaceResult {
        let result = clipboard::transaction(|snapshot| {
            if !selection.target.is_current() {
                return Err(ClipboardError::TargetChanged);
            }

            let before_copy = unsafe { GetClipboardSequenceNumber() };
            send_control_shortcut(VK_C).map_err(|_| ClipboardError::ShortcutUnavailable)?;
            if !wait_for_clipboard_change(before_copy) {
                return Err(ClipboardError::CopyTimedOut);
            }
            snapshot.mark_current_as_owned();

            if clipboard::read_unicode_text()?.as_deref() != Some(selection.text.as_str())
                || !selection.target.is_current()
            {
                return Err(ClipboardError::TargetChanged);
            }

            clipboard::write_unicode_text(replacement, snapshot)?;
            send_control_shortcut(VK_V).map_err(|_| ClipboardError::ShortcutUnavailable)?;
            thread::sleep(PASTE_SETTLE_TIME);
            Ok(())
        });

        match result {
            Ok(()) => ReplaceResult::Replaced,
            Err(ClipboardError::TargetChanged) => ReplaceResult::TargetChanged,
            Err(ClipboardError::CopyTimedOut) => ReplaceResult::TimedOut,
            Err(ClipboardError::Busy | ClipboardError::ShortcutUnavailable) => {
                ReplaceResult::TemporarilyUnavailable
            }
            Err(ClipboardError::ChangedExternally) => ReplaceResult::ClipboardChangedExternally,
            Err(ClipboardError::Restore(error)) => {
                ReplaceResult::Failure(ReplaceError::ClipboardRestore(error))
            }
            Err(error) => ReplaceResult::Failure(ReplaceError::Clipboard(error.to_string())),
        }
    }
}

fn wait_for_clipboard_change(previous: u32) -> bool {
    let deadline = Instant::now() + COPY_TIMEOUT;
    while Instant::now() < deadline {
        if unsafe { GetClipboardSequenceNumber() } != previous {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    false
}
