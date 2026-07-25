use std::{
    mem::size_of,
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    System::DataExchange::GetClipboardSequenceNumber,
    UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL,
    },
};

use crate::clipboard::{self, ClipboardError};

use super::{
    provider::SelectionProvider,
    result::{SelectionError, SelectionResult},
};

const COPY_TIMEOUT: Duration = Duration::from_millis(300);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(10);
const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

pub(super) struct ClipboardProvider;

impl SelectionProvider for ClipboardProvider {
    fn get_selected_text(&self) -> SelectionResult {
        match clipboard_selection() {
            Ok(Some(text)) if !text.is_empty() => SelectionResult::Success(text),
            Ok(_) => SelectionResult::NoSelection,
            Err(ClipboardError::Restore(error)) => {
                SelectionResult::Failure(SelectionError::ClipboardRestore(error))
            }
            Err(error) => SelectionResult::Failure(SelectionError::Clipboard(error.to_string())),
        }
    }
}

fn clipboard_selection() -> Result<Option<String>, ClipboardError> {
    clipboard::transaction(|_| {
        let sequence_before = unsafe { GetClipboardSequenceNumber() };
        send_copy_shortcut()?;
        if wait_for_clipboard_change(sequence_before) {
            clipboard::read_unicode_text()
        } else {
            Ok(None)
        }
    })
}

fn send_copy_shortcut() -> Result<(), ClipboardError> {
    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_C, false),
        keyboard_input(VK_C, true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(ClipboardError::Write(windows::core::Error::from_win32()));
    }
    Ok(())
}

fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
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
