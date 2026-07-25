//! Serialized, lossless clipboard transactions shared by platform services.

mod data_object;

use std::{
    mem::size_of,
    ptr,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use data_object::{ClipboardEntry, MaterializedDataObject};
use windows::{
    core::Error as WindowsError,
    Win32::{
        Foundation::{GlobalFree, HANDLE, S_FALSE},
        System::{
            Com::{
                CoTaskMemFree, IDataObject, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
            },
            DataExchange::{
                CloseClipboard, CountClipboardFormats, EmptyClipboard, OpenClipboard,
                SetClipboardData,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
            Ole::{
                OleFlushClipboard, OleGetClipboard, OleSetClipboard, ReleaseStgMedium,
                CF_UNICODETEXT,
            },
        },
    },
};

const RETRY_TIMEOUT: Duration = Duration::from_millis(500);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
static TRANSACTION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClipboardError {
    #[error("could not snapshot the clipboard: {0}")]
    Snapshot(#[source] WindowsError),
    #[error("could not read Unicode clipboard text: {0}")]
    Read(#[source] WindowsError),
    #[error("clipboard text had an invalid memory handle")]
    InvalidTextHandle,
    #[error("clipboard text was not valid UTF-16")]
    InvalidUtf16,
    #[error("could not write Unicode clipboard text: {0}")]
    Write(#[source] WindowsError),
    #[error("could not restore the clipboard: {0}")]
    Restore(#[source] WindowsError),
}

pub(crate) fn transaction<T>(
    operation: impl FnOnce(&ClipboardSnapshot) -> Result<T, ClipboardError>,
) -> Result<T, ClipboardError> {
    let _guard = TRANSACTION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let snapshot = ClipboardSnapshot::capture()?;
    let result = operation(&snapshot);
    let restoration = snapshot.restore();

    match (result, restoration) {
        (_, Err(error)) => Err(error),
        (result, Ok(())) => result,
    }
}

pub(crate) struct ClipboardSnapshot {
    data: Option<IDataObject>,
    restored: bool,
}

impl ClipboardSnapshot {
    fn capture() -> Result<Self, ClipboardError> {
        Ok(Self {
            data: materialize_clipboard()?,
            restored: false,
        })
    }

    fn restore(mut self) -> Result<(), ClipboardError> {
        self.restore_inner()?;
        self.restored = true;
        Ok(())
    }

    fn restore_inner(&self) -> Result<(), ClipboardError> {
        retry(|| unsafe {
            match &self.data {
                Some(data) => OleSetClipboard(data),
                None => OleSetClipboard(None::<&IDataObject>),
            }?;
            OleFlushClipboard()
        })
        .map_err(ClipboardError::Restore)
    }
}

impl Drop for ClipboardSnapshot {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore_inner();
        }
    }
}

pub(crate) fn read_unicode_text() -> Result<Option<String>, ClipboardError> {
    let data = retry(|| unsafe { OleGetClipboard() }).map_err(ClipboardError::Read)?;
    let format = FORMATETC {
        cfFormat: CF_UNICODETEXT.0,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let mut medium = match unsafe { data.GetData(&format) } {
        Ok(medium) => medium,
        Err(error) if error.code().0 == windows::Win32::Foundation::DV_E_FORMATETC.0 => {
            return Ok(None);
        }
        Err(error) => return Err(ClipboardError::Read(error)),
    };

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

pub(crate) fn write_unicode_text(text: &str) -> Result<(), ClipboardError> {
    let encoded: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = encoded.len() * size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) }.map_err(ClipboardError::Write)?;

    let pointer = unsafe { GlobalLock(memory) };
    if pointer.is_null() {
        unsafe {
            let _ = GlobalFree(Some(memory));
        }
        return Err(ClipboardError::InvalidTextHandle);
    }
    unsafe {
        ptr::copy_nonoverlapping(
            encoded.as_ptr().cast::<u8>(),
            pointer.cast::<u8>(),
            byte_len,
        );
        let _ = GlobalUnlock(memory);
    }

    let result = retry(|| unsafe {
        OpenClipboard(None)?;
        let operation = (|| {
            EmptyClipboard()?;
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        operation
    })
    .map_err(ClipboardError::Write);

    if result.is_err() {
        unsafe {
            let _ = GlobalFree(Some(memory));
        }
    }
    result
}

fn materialize_clipboard() -> Result<Option<IDataObject>, ClipboardError> {
    if unsafe { CountClipboardFormats() } == 0 {
        return Ok(None);
    }

    let source = retry(|| unsafe { OleGetClipboard() }).map_err(ClipboardError::Snapshot)?;
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

fn retry<T>(mut operation: impl FnMut() -> windows::core::Result<T>) -> windows::core::Result<T> {
    let deadline = Instant::now() + RETRY_TIMEOUT;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(_) if Instant::now() < deadline => thread::sleep(RETRY_INTERVAL),
            Err(error) => return Err(error),
        }
    }
}
