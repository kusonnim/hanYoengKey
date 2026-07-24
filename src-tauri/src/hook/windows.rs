use std::{
    sync::{
        mpsc::{self, Sender},
        Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
};

use thiserror::Error;
use windows::{
    core::Error as WindowsError,
    Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::VK_HANGUL,
            WindowsAndMessaging::{
                CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
                UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL,
                WM_KEYDOWN, WM_QUIT, WM_USER,
            },
        },
    },
};

use super::KeyboardEvent;

static EVENT_SENDER: OnceLock<Mutex<Option<Sender<KeyboardEvent>>>> = OnceLock::new();

#[derive(Debug, Error)]
pub(crate) enum HookError {
    #[error("failed to spawn the keyboard hook thread: {0}")]
    HookThread(#[source] std::io::Error),
    #[error("failed to install the global keyboard hook: {0}")]
    Install(#[source] WindowsError),
    #[error("a global keyboard hook is already installed")]
    AlreadyInstalled,
    #[error("keyboard hook initialization ended unexpectedly")]
    InitializationChannelClosed,
}

pub(crate) struct KeyboardHook {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl KeyboardHook {
    pub(crate) fn install(event_sender: Sender<KeyboardEvent>) -> Result<Self, HookError> {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("keyboard-hook".into())
            .spawn(move || hook_thread(event_sender, ready_sender))
            .map_err(HookError::HookThread)?;

        match ready_receiver.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(HookError::InitializationChannelClosed)
            }
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // Posting WM_QUIT lets the owning thread leave GetMessageW and unhook
        // from the same thread that installed the hook.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn hook_thread(
    event_sender: Sender<KeyboardEvent>,
    ready_sender: mpsc::SyncSender<Result<u32, HookError>>,
) {
    if set_event_sender(event_sender).is_err() {
        let _ = ready_sender.send(Err(HookError::AlreadyInstalled));
        return;
    }

    let hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) } {
        Ok(hook) => hook,
        Err(error) => {
            clear_event_sender();
            let _ = ready_sender.send(Err(HookError::Install(error)));
            return;
        }
    };

    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();

    // Force creation of this thread's message queue before publishing its ID.
    unsafe {
        let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
    }

    if ready_sender.send(Ok(thread_id)).is_err() {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
        clear_event_sender();
        return;
    }

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
    }

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
    clear_event_sender();
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam.0 as u32 == WM_KEYDOWN {
        let keyboard_input = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if let Some(event) = classify_event(code, wparam.0 as u32, keyboard_input.vkCode) {
            emit(event);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn classify_event(code: i32, message: u32, virtual_key: u32) -> Option<KeyboardEvent> {
    (code == HC_ACTION as i32 && message == WM_KEYDOWN && virtual_key == VK_HANGUL.0 as u32)
        .then_some(KeyboardEvent::HangulKeyPressed)
}

fn set_event_sender(sender: Sender<KeyboardEvent>) -> Result<(), ()> {
    let mut slot = EVENT_SENDER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if slot.is_some() {
        return Err(());
    }

    *slot = Some(sender);
    Ok(())
}

fn clear_event_sender() {
    if let Some(sender) = EVENT_SENDER.get() {
        *sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

fn emit(event: KeyboardEvent) {
    let sender = EVENT_SENDER.get().and_then(|sender| {
        sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    });

    if let Some(sender) = sender {
        let _ = sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_installs_and_uninstalls_cleanly() {
        let (sender, _receiver) = mpsc::channel();
        let hook = KeyboardHook::install(sender).expect("keyboard hook should install");
        drop(hook);

        let (sender, _receiver) = mpsc::channel();
        KeyboardHook::install(sender).expect("keyboard hook should reinstall after cleanup");
    }

    #[test]
    fn hangul_key_down_maps_to_high_level_event() {
        assert_eq!(
            classify_event(HC_ACTION as i32, WM_KEYDOWN, VK_HANGUL.0 as u32),
            Some(KeyboardEvent::HangulKeyPressed)
        );
    }
}
