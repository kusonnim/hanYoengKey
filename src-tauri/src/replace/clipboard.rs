use std::{mem::size_of, thread, time::Duration};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
};

use crate::clipboard::{self, ClipboardError};

use super::{
    provider::ReplaceProvider,
    result::{ReplaceError, ReplaceResult},
};

const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);
const PASTE_SETTLE_TIME: Duration = Duration::from_millis(50);

pub(super) struct ClipboardProvider;

impl ReplaceProvider for ClipboardProvider {
    fn replace_selected_text(&self, replacement: &str) -> ReplaceResult {
        match clipboard::transaction(|_| {
            clipboard::write_unicode_text(replacement)?;
            send_paste_shortcut()
                .map_err(|_| ClipboardError::Write(windows::core::Error::from_win32()))?;
            thread::sleep(PASTE_SETTLE_TIME);
            Ok(())
        }) {
            Ok(()) => ReplaceResult::Replaced,
            Err(ClipboardError::Restore(error)) => {
                ReplaceResult::Failure(ReplaceError::ClipboardRestore(error))
            }
            Err(ClipboardError::Write(_)) => ReplaceResult::TemporarilyUnavailable,
            Err(error) => ReplaceResult::Failure(ReplaceError::Clipboard(error.to_string())),
        }
    }
}

fn send_paste_shortcut() -> Result<(), ReplaceError> {
    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_V, false),
        keyboard_input(VK_V, true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(ReplaceError::PasteInputBlocked);
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
