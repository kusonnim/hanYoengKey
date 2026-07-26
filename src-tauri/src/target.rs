//! Provider-neutral identity for the control that owns a selection.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetIdentity {
    foreground_window: isize,
    focused_control: isize,
    process_id: u32,
    thread_id: u32,
}

impl TargetIdentity {
    pub(crate) fn capture() -> Option<Self> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return None;
        }

        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
        if thread_id == 0 {
            return None;
        }

        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let focused = if unsafe { GetGUIThreadInfo(thread_id, &mut info) }.is_ok()
            && !info.hwndFocus.0.is_null()
        {
            info.hwndFocus
        } else {
            foreground
        };

        Some(Self {
            foreground_window: foreground.0 as isize,
            focused_control: focused.0 as isize,
            process_id,
            thread_id,
        })
    }

    pub(crate) fn is_current(self) -> bool {
        Self::capture() == Some(self)
    }

    pub(crate) fn focused_window(self) -> HWND {
        HWND(self.focused_control as *mut core::ffi::c_void)
    }

    pub(crate) fn thread_id(self) -> u32 {
        self.thread_id
    }

    #[cfg(test)]
    pub(crate) fn test_identity() -> Self {
        Self {
            foreground_window: 1,
            focused_control: 2,
            process_id: 3,
            thread_id: 4,
        }
    }
}
