//! Selected-text retrieval through interchangeable providers.
//!
//! Consumers depend only on [`SelectionService`] and [`SelectionResult`].
//! Platform details and fallback policy remain private to this module.

mod provider;
mod result;

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod uia;

pub(crate) use result::{SelectionError, SelectionResult, SelectionSnapshot};

#[cfg(windows)]
use {
    crate::settings::ProviderPreference,
    clipboard::ClipboardProvider,
    provider::SelectionProvider,
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

    pub(crate) fn get_selected_text(
        &self,
        preference: ProviderPreference,
        debug_logging: bool,
    ) -> SelectionResult {
        let Some(target) = crate::target::TargetIdentity::capture() else {
            return SelectionResult::Unsupported;
        };
        let _apartment = match ComApartment::initialize() {
            Ok(apartment) => apartment,
            Err(error) => return SelectionResult::Failure(SelectionError::Com(error)),
        };

        let result = match preference {
            ProviderPreference::UiAutomationOnly => self.preferred.get_selected_text(),
            ProviderPreference::ClipboardOnly => self.fallback.get_selected_text(),
            ProviderPreference::Automatic => self.get_automatic(debug_logging),
        };

        match result {
            SelectionResult::Success(mut snapshot) => {
                let Some(current) = crate::target::TargetIdentity::capture() else {
                    return SelectionResult::TargetChanged;
                };
                if current != target {
                    return SelectionResult::TargetChanged;
                }
                snapshot.target = target;
                SelectionResult::Success(snapshot)
            }
            result => result,
        }
    }

    fn get_automatic(&self, debug_logging: bool) -> SelectionResult {
        match self.preferred.get_selected_text() {
            SelectionResult::Success(snapshot) => {
                if debug_logging {
                    eprintln!("[selection] provider=uia outcome=success");
                }
                SelectionResult::Success(snapshot)
            }
            SelectionResult::NoSelection => SelectionResult::NoSelection,
            SelectionResult::TargetChanged => SelectionResult::TargetChanged,
            SelectionResult::Unsupported => {
                if debug_logging {
                    eprintln!("[selection] provider=clipboard-fallback");
                }
                self.fallback.get_selected_text()
            }
            SelectionResult::TimedOut => SelectionResult::TimedOut,
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
