//! Selected-text retrieval through interchangeable providers.
//!
//! Consumers depend only on [`SelectionService`] and [`SelectionResult`].
//! Platform details and fallback policy remain private to this module.

mod provider;
mod result;

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod data_object;
#[cfg(windows)]
mod uia;

pub(crate) use result::SelectionResult;

#[cfg(windows)]
use {
    clipboard::ClipboardProvider,
    provider::SelectionProvider,
    result::SelectionError,
    uia::UiAutomationProvider,
    windows::Win32::System::Ole::{OleInitialize, OleUninitialize},
};

#[cfg(windows)]
pub(crate) struct SelectionService {
    preferred: UiAutomationProvider,
    fallback: ClipboardProvider,
}

#[cfg(windows)]
impl SelectionService {
    pub(crate) fn new() -> Self {
        Self {
            preferred: UiAutomationProvider,
            fallback: ClipboardProvider,
        }
    }

    pub(crate) fn get_selected_text(&self) -> SelectionResult {
        let _apartment = match ComApartment::initialize() {
            Ok(apartment) => apartment,
            Err(error) => return SelectionResult::Failure(SelectionError::Com(error)),
        };

        match self.preferred.get_selected_text() {
            result @ (SelectionResult::Success(_) | SelectionResult::NoSelection) => result,
            SelectionResult::Unsupported => self.fallback.get_selected_text(),
            SelectionResult::Failure(preferred_error) => match self.fallback.get_selected_text() {
                SelectionResult::Failure(fallback_error) => {
                    SelectionResult::Failure(SelectionError::ProvidersFailed {
                        preferred: preferred_error.to_string(),
                        fallback: fallback_error.to_string(),
                    })
                }
                result => result,
            },
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
