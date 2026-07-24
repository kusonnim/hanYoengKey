use std::{
    mem::size_of,
    ptr, thread,
    time::{Duration, Instant},
};

use windows::{
    core::Error as WindowsError,
    Win32::{
        Foundation::S_FALSE,
        System::{
            Com::{
                CoTaskMemFree, IDataObject, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
            },
            DataExchange::{CountClipboardFormats, GetClipboardSequenceNumber},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                OleFlushClipboard, OleGetClipboard, OleSetClipboard, ReleaseStgMedium,
                CF_UNICODETEXT,
            },
        },
        UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
            VK_CONTROL,
        },
    },
};

use super::{
    data_object::{ClipboardEntry, MaterializedDataObject},
    provider::SelectionProvider,
    result::{SelectionError, SelectionResult},
};

const COPY_TIMEOUT: Duration = Duration::from_millis(300);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLIPBOARD_RETRY_TIMEOUT: Duration = Duration::from_millis(500);
const CLIPBOARD_RETRY_INTERVAL: Duration = Duration::from_millis(10);
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

#[derive(Debug, thiserror::Error)]
enum ClipboardError {
    #[error("could not snapshot the clipboard: {0}")]
    Snapshot(#[source] WindowsError),
    #[error("Ctrl+C input injection was blocked")]
    CopyInputBlocked,
    #[error("could not read copied clipboard data: {0}")]
    Read(#[source] WindowsError),
    #[error("copied clipboard text had an invalid memory handle")]
    InvalidTextHandle,
    #[error("copied clipboard text was not valid UTF-16")]
    InvalidUtf16,
    #[error("could not restore the clipboard: {0}")]
    Restore(#[source] WindowsError),
}

fn clipboard_selection() -> Result<Option<String>, ClipboardError> {
    let snapshot = ClipboardSnapshot::capture()?;
    let sequence_before = unsafe { GetClipboardSequenceNumber() };

    send_copy_shortcut()?;
    let changed = wait_for_clipboard_change(sequence_before);
    let selection = if changed {
        read_unicode_text()
    } else {
        Ok(None)
    };
    let restoration = snapshot.restore();

    match (selection, restoration) {
        (_, Err(error)) => Err(error),
        (result, Ok(())) => result,
    }
}

struct ClipboardSnapshot {
    data: Option<IDataObject>,
    restored: bool,
}

impl ClipboardSnapshot {
    fn capture() -> Result<Self, ClipboardError> {
        let data = materialize_clipboard()?;

        Ok(Self {
            data,
            restored: false,
        })
    }

    fn restore(mut self) -> Result<(), ClipboardError> {
        self.restore_inner()?;
        self.restored = true;
        Ok(())
    }

    fn restore_inner(&self) -> Result<(), ClipboardError> {
        retry_clipboard(|| unsafe {
            match &self.data {
                Some(data) => OleSetClipboard(data),
                None => OleSetClipboard(None::<&IDataObject>),
            }?;
            OleFlushClipboard()
        })
        .map_err(ClipboardError::Restore)
    }
}

fn materialize_clipboard() -> Result<Option<IDataObject>, ClipboardError> {
    if unsafe { CountClipboardFormats() } == 0 {
        return Ok(None);
    }

    let source =
        retry_clipboard(|| unsafe { OleGetClipboard() }).map_err(ClipboardError::Snapshot)?;
    let formats = unsafe {
        source
            .EnumFormatEtc(DATADIR_GET.0 as u32)
            .map_err(ClipboardError::Snapshot)?
    };
    let mut entries = Vec::new();

    loop {
        let mut format = FORMATETC::default();
        let mut fetched = 0;
        let status = unsafe { formats.Next(std::slice::from_mut(&mut format), Some(&mut fetched)) };

        if status == S_FALSE || fetched == 0 {
            break;
        }
        status.ok().map_err(ClipboardError::Snapshot)?;

        let medium = match unsafe { source.GetData(&format) } {
            Ok(medium) => medium,
            Err(error) => {
                free_format_target_device(&mut format);
                return Err(ClipboardError::Snapshot(error));
            }
        };

        entries.push(ClipboardEntry::new(format, medium));
    }

    Ok(Some(MaterializedDataObject::new(entries).into()))
}

fn free_format_target_device(format: &mut FORMATETC) {
    if !format.ptd.is_null() {
        unsafe {
            CoTaskMemFree(Some(format.ptd.cast()));
        }
        format.ptd = ptr::null_mut();
    }
}

impl Drop for ClipboardSnapshot {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore_inner();
        }
    }
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
        return Err(ClipboardError::CopyInputBlocked);
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

fn read_unicode_text() -> Result<Option<String>, ClipboardError> {
    let data = retry_clipboard(|| unsafe { OleGetClipboard() }).map_err(ClipboardError::Read)?;
    let format = FORMATETC {
        cfFormat: CF_UNICODETEXT.0,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let mut medium = unsafe { data.GetData(&format).map_err(ClipboardError::Read)? };

    let result = unsafe {
        let global = medium.u.hGlobal;
        let pointer = GlobalLock(global);
        if pointer.is_null() {
            Err(ClipboardError::InvalidTextHandle)
        } else {
            let units = GlobalSize(global) / size_of::<u16>();
            let slice = std::slice::from_raw_parts(pointer.cast::<u16>(), units);
            let length = slice.iter().position(|unit| *unit == 0).unwrap_or(units);
            let text =
                String::from_utf16(&slice[..length]).map_err(|_| ClipboardError::InvalidUtf16);
            let _ = GlobalUnlock(global);
            text.map(Some)
        }
    };

    unsafe {
        ReleaseStgMedium(&mut medium);
    }

    result
}

fn retry_clipboard<T>(
    mut operation: impl FnMut() -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    let deadline = Instant::now() + CLIPBOARD_RETRY_TIMEOUT;

    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(_) if Instant::now() < deadline => thread::sleep(CLIPBOARD_RETRY_INTERVAL),
            Err(error) => return Err(error),
        }
    }
}
