use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Registry::{
            RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            REG_SZ,
        },
    },
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "HanYeongKey";

pub(super) trait StartupRegistration: Send + Sync {
    fn set_enabled(&self, enabled: bool) -> Result<(), StartupError>;
}

pub(super) struct UnavailableStartupRegistration;

impl StartupRegistration for UnavailableStartupRegistration {
    fn set_enabled(&self, _enabled: bool) -> Result<(), StartupError> {
        Err(StartupError::Unavailable)
    }
}

pub(super) struct WindowsStartupRegistration {
    executable: Box<Path>,
}

impl WindowsStartupRegistration {
    pub(super) fn current_executable() -> Result<Self, StartupError> {
        Ok(Self {
            executable: std::env::current_exe()
                .map_err(StartupError::Executable)?
                .into_boxed_path(),
        })
    }
}

impl StartupRegistration for WindowsStartupRegistration {
    fn set_enabled(&self, enabled: bool) -> Result<(), StartupError> {
        let key_path = wide(RUN_KEY);
        let value_name = wide(VALUE_NAME);
        let mut key = HKEY::default();
        unsafe {
            RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(key_path.as_ptr()), &mut key)
                .ok()
                .map_err(StartupError::Registry)?;
        }
        let key_guard = RegistryKey(key);

        if enabled {
            let command = format!("\"{}\"", self.executable.display());
            let encoded = wide(&command);
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    encoded.as_ptr().cast::<u8>(),
                    encoded.len() * std::mem::size_of::<u16>(),
                )
            };
            unsafe {
                RegSetValueExW(
                    key_guard.0,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(bytes),
                )
                .ok()
                .map_err(StartupError::Registry)?;
            }
        } else {
            let status = unsafe { RegDeleteValueW(key_guard.0, PCWSTR(value_name.as_ptr())) };
            if status != ERROR_FILE_NOT_FOUND {
                status.ok().map_err(StartupError::Registry)?;
            }
        }
        Ok(())
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StartupError {
    #[error("could not resolve the application executable: {0}")]
    Executable(#[source] std::io::Error),
    #[error("Windows startup registration failed: {0}")]
    Registry(#[source] windows::core::Error),
    #[error("Windows startup registration is unavailable")]
    Unavailable,
    #[cfg(test)]
    #[error("mock startup registration failure")]
    Mock,
}
