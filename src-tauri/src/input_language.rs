//! Windows input-language synchronization for a captured target context.

use std::{
    cell::RefCell,
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Input::{
            Ime::{ImmGetContext, ImmGetOpenStatus, ImmReleaseContext, ImmSetOpenStatus},
            KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList, HKL, VK_HANGUL},
        },
        WindowsAndMessaging::{PostMessageW, WM_INPUTLANGCHANGEREQUEST},
    },
};

use crate::{input::send_key_press, target::TargetIdentity};

const PRIMARY_LANGUAGE_ENGLISH: u16 = 0x09;
const PRIMARY_LANGUAGE_KOREAN: u16 = 0x12;
const LAYOUT_CHANGE_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputLanguage {
    English,
    Korean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnsureInputLanguageResult {
    AlreadySet,
    Changed,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputLanguageError {
    #[error("the captured input target is no longer active")]
    TargetChanged,
    #[error("the target input context is unavailable")]
    ContextUnavailable,
    #[error("the target input language could not be queried")]
    QueryFailed,
    #[error("the requested keyboard layout is not installed")]
    LayoutUnavailable,
    #[error("the target rejected the input-language change")]
    ChangeRejected,
    #[error("the target did not apply the input-language change in time")]
    ChangeTimedOut,
}

pub(crate) trait InputLanguageBackend {
    fn get(&self, target: &TargetIdentity) -> Result<InputLanguage, InputLanguageError>;
    fn set(
        &self,
        target: &TargetIdentity,
        desired: InputLanguage,
    ) -> Result<(), InputLanguageError>;
}

pub(crate) struct InputLanguageService<B = WindowsInputLanguageBackend> {
    backend: B,
}

impl InputLanguageService {
    pub(crate) fn new() -> Self {
        Self {
            backend: WindowsInputLanguageBackend,
        }
    }
}

impl<B: InputLanguageBackend> InputLanguageService<B> {
    pub(crate) fn get_input_language(
        &self,
        target: &TargetIdentity,
    ) -> Result<InputLanguage, InputLanguageError> {
        self.backend.get(target)
    }

    pub(crate) fn ensure_input_language(
        &self,
        target: &TargetIdentity,
        desired: InputLanguage,
    ) -> Result<EnsureInputLanguageResult, InputLanguageError> {
        if self.get_input_language(target)? == desired {
            return Ok(EnsureInputLanguageResult::AlreadySet);
        }
        self.backend.set(target, desired)?;
        Ok(EnsureInputLanguageResult::Changed)
    }
}

pub(crate) struct WindowsInputLanguageBackend;

impl InputLanguageBackend for WindowsInputLanguageBackend {
    fn get(&self, target: &TargetIdentity) -> Result<InputLanguage, InputLanguageError> {
        ensure_target(target)?;
        let layout = unsafe { GetKeyboardLayout(target.thread_id()) };
        match primary_language(layout) {
            PRIMARY_LANGUAGE_KOREAN => {
                let context = ImeContext::acquire(target)?;
                Ok(language_from_korean_ime_open(
                    unsafe { ImmGetOpenStatus(context.handle) }.as_bool(),
                ))
            }
            PRIMARY_LANGUAGE_ENGLISH => Ok(InputLanguage::English),
            _ => Err(InputLanguageError::QueryFailed),
        }
    }

    fn set(
        &self,
        target: &TargetIdentity,
        desired: InputLanguage,
    ) -> Result<(), InputLanguageError> {
        ensure_target(target)?;
        let current_layout = unsafe { GetKeyboardLayout(target.thread_id()) };

        if primary_language(current_layout) == PRIMARY_LANGUAGE_KOREAN {
            return ensure_korean_ime_state(self, target, desired);
        }

        let desired_primary = match desired {
            InputLanguage::English => PRIMARY_LANGUAGE_ENGLISH,
            InputLanguage::Korean => PRIMARY_LANGUAGE_KOREAN,
        };
        let layout =
            find_installed_layout(desired_primary).ok_or(InputLanguageError::LayoutUnavailable)?;

        ensure_target(target)?;
        unsafe {
            PostMessageW(
                Some(target.focused_window()),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(layout.0 as isize),
            )
        }
        .map_err(|_| InputLanguageError::ChangeRejected)?;

        let deadline = Instant::now() + LAYOUT_CHANGE_TIMEOUT;
        loop {
            ensure_target(target)?;
            let applied = unsafe { GetKeyboardLayout(target.thread_id()) };
            if primary_language(applied) == desired_primary {
                break;
            }
            if Instant::now() >= deadline {
                return Err(InputLanguageError::ChangeTimedOut);
            }
            thread::sleep(Duration::from_millis(10));
        }

        if desired == InputLanguage::Korean {
            ensure_korean_ime_state(self, target, desired)?;
        }
        verify_language(self, target, desired)
    }
}

fn ensure_target(target: &TargetIdentity) -> Result<(), InputLanguageError> {
    target
        .is_current()
        .then_some(())
        .ok_or(InputLanguageError::TargetChanged)
}

fn primary_language(layout: HKL) -> u16 {
    (layout.0 as usize as u16) & 0x03ff
}

fn find_installed_layout(primary: u16) -> Option<HKL> {
    let count = unsafe { GetKeyboardLayoutList(None) };
    if count <= 0 {
        return None;
    }
    let mut layouts = vec![HKL::default(); count as usize];
    let copied = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) };
    layouts
        .into_iter()
        .take(copied.max(0) as usize)
        .find(|layout| primary_language(*layout) == primary)
}

fn language_from_korean_ime_open(open: bool) -> InputLanguage {
    if open {
        InputLanguage::Korean
    } else {
        InputLanguage::English
    }
}

fn desired_ime_open(desired: InputLanguage) -> bool {
    desired == InputLanguage::Korean
}

fn set_korean_ime_open(
    target: &TargetIdentity,
    desired: InputLanguage,
) -> Result<(), InputLanguageError> {
    ensure_target(target)?;
    let context = ImeContext::acquire(target)?;
    unsafe { ImmSetOpenStatus(context.handle, desired_ime_open(desired)) }
        .as_bool()
        .then_some(())
        .ok_or(InputLanguageError::ChangeRejected)?;
    ensure_target(target)?;

    let applied =
        language_from_korean_ime_open(unsafe { ImmGetOpenStatus(context.handle) }.as_bool());
    (applied == desired)
        .then_some(())
        .ok_or(InputLanguageError::ChangeRejected)
}

fn ensure_korean_ime_state(
    backend: &WindowsInputLanguageBackend,
    target: &TargetIdentity,
    desired: InputLanguage,
) -> Result<(), InputLanguageError> {
    if backend.get(target)? == desired {
        return Ok(());
    }

    if set_korean_ime_open(target, desired).is_ok() {
        return Ok(());
    }

    // Recent Windows Korean IMEs may ignore ImmSetOpenStatus even though the
    // call succeeds. A single marked Hangul-key input is used only after a
    // verified mismatch and only while the captured target is still current.
    ensure_target(target)?;
    send_key_press(VK_HANGUL).map_err(|_| InputLanguageError::ChangeRejected)?;
    wait_for_language(backend, target, desired)
}

fn wait_for_language(
    backend: &WindowsInputLanguageBackend,
    target: &TargetIdentity,
    desired: InputLanguage,
) -> Result<(), InputLanguageError> {
    let deadline = Instant::now() + LAYOUT_CHANGE_TIMEOUT;
    loop {
        ensure_target(target)?;
        if backend.get(target)? == desired {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(InputLanguageError::ChangeTimedOut);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn verify_language(
    backend: &WindowsInputLanguageBackend,
    target: &TargetIdentity,
    desired: InputLanguage,
) -> Result<(), InputLanguageError> {
    (backend.get(target)? == desired)
        .then_some(())
        .ok_or(InputLanguageError::ChangeRejected)
}

struct ImeContext {
    window: windows::Win32::Foundation::HWND,
    handle: windows::Win32::UI::Input::Ime::HIMC,
    _not_sync: RefCell<()>,
}

impl ImeContext {
    fn acquire(target: &TargetIdentity) -> Result<Self, InputLanguageError> {
        let window = target.focused_window();
        let handle = unsafe { ImmGetContext(window) };
        if handle.0.is_null() {
            return Err(InputLanguageError::ContextUnavailable);
        }
        Ok(Self {
            window,
            handle,
            _not_sync: RefCell::new(()),
        })
    }
}

impl Drop for ImeContext {
    fn drop(&mut self) {
        unsafe {
            let _ = ImmReleaseContext(self.window, self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct FakeBackend {
        current: InputLanguage,
        set_calls: Cell<usize>,
        set_error: Option<InputLanguageError>,
    }

    impl InputLanguageBackend for FakeBackend {
        fn get(&self, _target: &TargetIdentity) -> Result<InputLanguage, InputLanguageError> {
            Ok(self.current)
        }

        fn set(
            &self,
            _target: &TargetIdentity,
            _desired: InputLanguage,
        ) -> Result<(), InputLanguageError> {
            self.set_calls.set(self.set_calls.get() + 1);
            self.set_error.map_or(Ok(()), Err)
        }
    }

    #[test]
    fn does_not_change_an_already_matching_language() {
        let service = InputLanguageService {
            backend: FakeBackend {
                current: InputLanguage::Korean,
                set_calls: Cell::new(0),
                set_error: None,
            },
        };
        assert_eq!(
            service.ensure_input_language(&TargetIdentity::test_identity(), InputLanguage::Korean),
            Ok(EnsureInputLanguageResult::AlreadySet)
        );
        assert_eq!(service.backend.set_calls.get(), 0);
    }

    #[test]
    fn target_change_prevents_backend_modification() {
        let service = InputLanguageService {
            backend: FakeBackend {
                current: InputLanguage::English,
                set_calls: Cell::new(0),
                set_error: Some(InputLanguageError::TargetChanged),
            },
        };
        assert_eq!(
            service.ensure_input_language(&TargetIdentity::test_identity(), InputLanguage::Korean),
            Err(InputLanguageError::TargetChanged)
        );
        assert_eq!(service.backend.set_calls.get(), 1);
    }

    #[test]
    fn korean_ime_open_state_maps_to_hangul_mode() {
        assert_eq!(language_from_korean_ime_open(true), InputLanguage::Korean);
        assert_eq!(language_from_korean_ime_open(false), InputLanguage::English);
        assert!(desired_ime_open(InputLanguage::Korean));
        assert!(!desired_ime_open(InputLanguage::English));
    }
}
