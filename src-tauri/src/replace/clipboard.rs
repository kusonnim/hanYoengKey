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

            let Some(current_selection) = clipboard::read_unicode_text()? else {
                return Err(ClipboardError::TargetChanged);
            };
            if !equivalent_selection_text(&current_selection, &selection.text)
                || !selection.target.is_current()
            {
                return Err(ClipboardError::TargetChanged);
            }

            // UI Automation and the clipboard can expose the same Windows
            // selection with different newline encodings. The clipboard copy
            // is authoritative for replacement, so retain its exact sequence.
            let replacement = preserve_line_endings(replacement, &current_selection)
                .ok_or(ClipboardError::TargetChanged)?;

            clipboard::write_unicode_text(&replacement, snapshot)?;
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

fn equivalent_selection_text(left: &str, right: &str) -> bool {
    canonical_line_endings(left) == canonical_line_endings(right)
}

fn canonical_line_endings(text: &str) -> String {
    let mut canonical = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            canonical.push('\n');
        } else {
            canonical.push(character);
        }
    }
    canonical
}

fn preserve_line_endings(replacement: &str, selected: &str) -> Option<String> {
    let selected_endings = line_endings(selected);
    let replacement_endings = line_endings(replacement);
    if selected_endings.len() != replacement_endings.len() {
        return None;
    }

    let mut output = String::with_capacity(replacement.len());
    let mut ending_index = 0;
    let mut segment_start = 0;
    let bytes = replacement.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' || bytes[index] == b'\n' {
            output.push_str(&replacement[segment_start..index]);
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            output.push_str(selected_endings[ending_index]);
            ending_index += 1;
            segment_start = index + 1;
        }
        index += 1;
    }
    output.push_str(&replacement[segment_start..]);
    Some(output)
}

fn line_endings(text: &str) -> Vec<&str> {
    let mut endings = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                endings.push(&text[index..index + 2]);
                index += 2;
            }
            b'\r' | b'\n' => {
                endings.push(&text[index..index + 1]);
                index += 1;
            }
            _ => index += 1,
        }
    }
    endings
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_validation_accepts_equivalent_windows_line_endings() {
        assert!(equivalent_selection_text(
            "first\r\n\r\nsecond\rthird",
            "first\n\nsecond\nthird"
        ));
        assert!(!equivalent_selection_text(
            "first\r\nsecond",
            "first\nchanged"
        ));
    }

    #[test]
    fn replacement_uses_the_copied_selections_exact_line_endings() {
        assert_eq!(
            preserve_line_endings(
                "안녕하세요\n\n과\r동이\r\n",
                "dkssudgktpdy\r\n\r\nrhk\nehddl\r"
            ),
            Some("안녕하세요\r\n\r\n과\n동이\r".into())
        );
    }

    #[test]
    fn replacement_rejects_a_different_line_count() {
        assert_eq!(preserve_line_endings("one\ntwo", "one"), None);
    }
}
