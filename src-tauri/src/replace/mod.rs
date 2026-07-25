//! Replacement of the active selection through interchangeable providers.

mod provider;
mod result;

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod uia;

pub(crate) use result::{ReplaceError, ReplaceResult};

#[cfg(windows)]
use {
    crate::selection::SelectionSnapshot,
    clipboard::ClipboardProvider,
    provider::ReplaceProvider,
    uia::UiAutomationProvider,
    windows::Win32::System::Ole::{OleInitialize, OleUninitialize},
};

#[cfg(windows)]
pub(crate) struct ReplaceService {
    preferred: UiAutomationProvider,
    fallback: ClipboardProvider,
}

#[cfg(windows)]
impl ReplaceService {
    pub(crate) fn new() -> Self {
        Self {
            preferred: UiAutomationProvider,
            fallback: ClipboardProvider,
        }
    }

    pub(crate) fn replace_selected_text(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
    ) -> ReplaceResult {
        if !selection.target.is_current() {
            return ReplaceResult::TargetChanged;
        }
        let _apartment = match ComApartment::initialize() {
            Ok(apartment) => apartment,
            Err(error) => return ReplaceResult::Failure(ReplaceError::Com(error)),
        };

        match self.preferred.replace_selected_text(selection, replacement) {
            ReplaceResult::Unsupported => {
                eprintln!("[replace] provider=clipboard-fallback");
                self.fallback.replace_selected_text(selection, replacement)
            }
            ReplaceResult::Failure(preferred_error) => {
                match self.fallback.replace_selected_text(selection, replacement) {
                    ReplaceResult::Failure(fallback_error) => {
                        ReplaceResult::Failure(ReplaceError::ProvidersFailed {
                            preferred: preferred_error.to_string(),
                            fallback: fallback_error.to_string(),
                        })
                    }
                    result => result,
                }
            }
            result => result,
        }
    }
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl ComApartment {
    fn initialize() -> windows::core::Result<Self> {
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}
