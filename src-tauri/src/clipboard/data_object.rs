use std::sync::Mutex;

use windows::{
    core::{implement, Error, Result as WindowsResult},
    Win32::{
        Foundation::{DV_E_FORMATETC, E_NOTIMPL, OLE_E_ADVISENOTSUPPORTED, S_FALSE, S_OK},
        System::{
            Com::Urlmon::CopyStgMedium,
            Com::{
                CoTaskMemAlloc, CoTaskMemFree, IAdviseSink, IDataObject, IDataObject_Impl,
                IEnumFORMATETC, IEnumFORMATETC_Impl, IEnumSTATDATA, DATADIR_GET, FORMATETC,
                STGMEDIUM,
            },
            Ole::ReleaseStgMedium,
        },
    },
};

pub(super) struct ClipboardEntry {
    format: FORMATETC,
    medium: STGMEDIUM,
}

impl ClipboardEntry {
    pub(super) fn new(format: FORMATETC, medium: STGMEDIUM) -> Self {
        Self { format, medium }
    }
}

impl Drop for ClipboardEntry {
    fn drop(&mut self) {
        unsafe {
            ReleaseStgMedium(&mut self.medium);
        }
        free_target_device(&mut self.format);
    }
}

#[implement(IDataObject)]
pub(super) struct MaterializedDataObject {
    entries: Vec<ClipboardEntry>,
}

impl MaterializedDataObject {
    pub(super) fn new(entries: Vec<ClipboardEntry>) -> Self {
        Self { entries }
    }
}

impl IDataObject_Impl for MaterializedDataObject_Impl {
    fn GetData(&self, requested: *const FORMATETC) -> WindowsResult<STGMEDIUM> {
        let requested = unsafe { requested.as_ref() }.ok_or_else(format_error)?;
        let entry = self
            .entries
            .iter()
            .find(|entry| format_matches(&entry.format, requested))
            .ok_or_else(format_error)?;

        unsafe { CopyStgMedium(&entry.medium) }
    }

    fn GetDataHere(&self, _format: *const FORMATETC, _medium: *mut STGMEDIUM) -> WindowsResult<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn QueryGetData(&self, requested: *const FORMATETC) -> windows::core::HRESULT {
        let Some(requested) = (unsafe { requested.as_ref() }) else {
            return DV_E_FORMATETC;
        };

        if self
            .entries
            .iter()
            .any(|entry| format_matches(&entry.format, requested))
        {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _input: *const FORMATETC,
        output: *mut FORMATETC,
    ) -> windows::core::HRESULT {
        if let Some(output) = unsafe { output.as_mut() } {
            output.ptd = std::ptr::null_mut();
        }
        E_NOTIMPL
    }

    fn SetData(
        &self,
        _format: *const FORMATETC,
        _medium: *const STGMEDIUM,
        _release: windows::core::BOOL,
    ) -> WindowsResult<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn EnumFormatEtc(&self, direction: u32) -> WindowsResult<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(Error::from_hresult(E_NOTIMPL));
        }

        let formats = self
            .entries
            .iter()
            .map(|entry| clone_format(&entry.format))
            .collect::<WindowsResult<Vec<_>>>()?;

        Ok(FormatEnumerator::new(formats).into())
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _flags: u32,
        _sink: windows::core::Ref<'_, IAdviseSink>,
    ) -> WindowsResult<u32> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _connection: u32) -> WindowsResult<()> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn EnumDAdvise(&self) -> WindowsResult<IEnumSTATDATA> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }
}

#[implement(IEnumFORMATETC)]
struct FormatEnumerator {
    state: Mutex<EnumeratorState>,
}

struct EnumeratorState {
    formats: Vec<FORMATETC>,
    cursor: usize,
}

impl FormatEnumerator {
    fn new(formats: Vec<FORMATETC>) -> Self {
        Self {
            state: Mutex::new(EnumeratorState { formats, cursor: 0 }),
        }
    }
}

impl IEnumFORMATETC_Impl for FormatEnumerator_Impl {
    fn Next(
        &self,
        count: u32,
        output: *mut FORMATETC,
        fetched: *mut u32,
    ) -> windows::core::HRESULT {
        if output.is_null() {
            return E_NOTIMPL;
        }

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let available = state.formats.len().saturating_sub(state.cursor);
        let copied = available.min(count as usize);

        for offset in 0..copied {
            let format = match clone_format(&state.formats[state.cursor + offset]) {
                Ok(format) => format,
                Err(error) => return error.code(),
            };
            unsafe {
                output.add(offset).write(format);
            }
        }

        state.cursor += copied;
        if !fetched.is_null() {
            unsafe {
                fetched.write(copied as u32);
            }
        }

        if copied == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, count: u32) -> WindowsResult<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cursor = (state.cursor + count as usize).min(state.formats.len());
        Ok(())
    }

    fn Reset(&self) -> WindowsResult<()> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cursor = 0;
        Ok(())
    }

    fn Clone(&self) -> WindowsResult<IEnumFORMATETC> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let formats = state
            .formats
            .iter()
            .map(clone_format)
            .collect::<WindowsResult<Vec<_>>>()?;
        let clone = FormatEnumerator::new(formats);
        clone
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cursor = state.cursor;
        Ok(clone.into())
    }
}

impl Drop for FormatEnumerator {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for format in &mut state.formats {
            free_target_device(format);
        }
    }
}

fn format_matches(stored: &FORMATETC, requested: &FORMATETC) -> bool {
    stored.cfFormat == requested.cfFormat
        && stored.dwAspect == requested.dwAspect
        && stored.lindex == requested.lindex
        && stored.tymed & requested.tymed != 0
}

fn clone_format(source: &FORMATETC) -> WindowsResult<FORMATETC> {
    let mut cloned = *source;
    cloned.ptd = clone_target_device(source.ptd)?;
    Ok(cloned)
}

fn clone_target_device(
    source: *mut windows::Win32::System::Com::DVTARGETDEVICE,
) -> WindowsResult<*mut windows::Win32::System::Com::DVTARGETDEVICE> {
    if source.is_null() {
        return Ok(std::ptr::null_mut());
    }

    let bytes = unsafe { (*source).tdSize as usize };
    let destination = unsafe { CoTaskMemAlloc(bytes) };
    if destination.is_null() {
        return Err(Error::from_hresult(
            windows::Win32::Foundation::E_OUTOFMEMORY,
        ));
    }

    unsafe {
        std::ptr::copy_nonoverlapping(source.cast::<u8>(), destination.cast::<u8>(), bytes);
    }
    Ok(destination.cast())
}

fn free_target_device(format: &mut FORMATETC) {
    if !format.ptd.is_null() {
        unsafe {
            CoTaskMemFree(Some(format.ptd.cast()));
        }
        format.ptd = std::ptr::null_mut();
    }
}

fn format_error() -> Error {
    Error::from_hresult(DV_E_FORMATETC)
}
