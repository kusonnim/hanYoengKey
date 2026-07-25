//! Safe, marked synthetic keyboard shortcuts.

use std::mem::size_of;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

pub(crate) const SYNTHETIC_INPUT_MARKER: usize = 0x4841_4E59_454F_4E47;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutError {
    ConflictingModifier,
    InjectionBlocked,
}

pub(crate) fn send_control_shortcut(key: VIRTUAL_KEY) -> Result<(), ShortcutError> {
    if [VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(key_is_down)
    {
        return Err(ShortcutError::ConflictingModifier);
    }

    let control_was_down = key_is_down(VK_CONTROL);
    let mut inputs = Vec::with_capacity(4);
    if !control_was_down {
        inputs.push(keyboard_input(VK_CONTROL, false));
    }
    inputs.push(keyboard_input(key, false));
    inputs.push(keyboard_input(key, true));
    if !control_was_down {
        inputs.push(keyboard_input(VK_CONTROL, true));
    }

    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } as usize;
    if sent != inputs.len() {
        // Always release keys introduced by this utility after a partial send.
        let mut cleanup = vec![keyboard_input(key, true)];
        if !control_was_down {
            cleanup.push(keyboard_input(VK_CONTROL, true));
        }
        unsafe {
            let _ = SendInput(&cleanup, size_of::<INPUT>() as i32);
        }
        return Err(ShortcutError::InjectionBlocked);
    }
    Ok(())
}

pub(crate) fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
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
                dwExtraInfo: SYNTHETIC_INPUT_MARKER,
                ..Default::default()
            },
        },
    }
}

fn key_is_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(key.0 as i32) }) < 0
}
