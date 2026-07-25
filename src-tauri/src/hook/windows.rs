use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
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
            Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_HANGUL,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
                UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL,
                WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER,
            },
        },
    },
};

use super::KeyboardEvent;

static EVENT_SENDER: OnceLock<Mutex<Option<Sender<KeyboardEvent>>>> = OnceLock::new();
static FLOW_ACTIVE: AtomicBool = AtomicBool::new(false);
static PHYSICAL_KEY_DOWN: AtomicBool = AtomicBool::new(false);
static SUPPRESS_KEY_UP: AtomicBool = AtomicBool::new(false);
static REPLAY_ON_KEY_UP: AtomicBool = AtomicBool::new(false);
const SYNTHETIC_INPUT_MARKER: usize = 0x4841_4E59_454F_4E47;

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
    if code == HC_ACTION as i32 {
        let keyboard_input = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let is_activation = classify_event(
            code,
            message,
            keyboard_input.vkCode,
            keyboard_input.dwExtraInfo,
        )
        .is_some();
        let is_physical_release = matches!(message, WM_KEYUP | WM_SYSKEYUP)
            && keyboard_input.vkCode == VK_HANGUL.0 as u32
            && keyboard_input.dwExtraInfo != SYNTHETIC_INPUT_MARKER;
        if is_activation || is_physical_release {
            match message {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    if PHYSICAL_KEY_DOWN.swap(true, Ordering::AcqRel) {
                        return LRESULT(1);
                    }
                    if FLOW_ACTIVE
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        if emit(KeyboardEvent::HangulKeyPressed) {
                            SUPPRESS_KEY_UP.store(true, Ordering::Release);
                            return LRESULT(1);
                        }
                        FLOW_ACTIVE.store(false, Ordering::Release);
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    PHYSICAL_KEY_DOWN.store(false, Ordering::Release);
                    if SUPPRESS_KEY_UP.swap(false, Ordering::AcqRel) {
                        if REPLAY_ON_KEY_UP.swap(false, Ordering::AcqRel) {
                            replay_hangul_key();
                        }
                        return LRESULT(1);
                    }
                }
                _ => {}
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn classify_event(
    code: i32,
    message: u32,
    virtual_key: u32,
    extra_info: usize,
) -> Option<KeyboardEvent> {
    (code == HC_ACTION as i32
        && matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
        && virtual_key == VK_HANGUL.0 as u32
        && extra_info != SYNTHETIC_INPUT_MARKER)
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

fn emit(event: KeyboardEvent) -> bool {
    let sender = EVENT_SENDER.get().and_then(|sender| {
        sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    });

    if let Some(sender) = sender {
        sender.send(event).is_ok()
    } else {
        false
    }
}

pub(crate) fn complete_event(handled: bool) {
    FLOW_ACTIVE.store(false, Ordering::Release);
    if handled {
        REPLAY_ON_KEY_UP.store(false, Ordering::Release);
    } else if PHYSICAL_KEY_DOWN.load(Ordering::Acquire) {
        REPLAY_ON_KEY_UP.store(true, Ordering::Release);
    } else {
        replay_hangul_key();
    }
}

fn replay_hangul_key() {
    let inputs = [hangul_input(false), hangul_input(true)];
    unsafe {
        let _ = SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

fn hangul_input(key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_HANGUL,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                dwExtraInfo: SYNTHETIC_INPUT_MARKER,
                ..Default::default()
            },
        },
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
            classify_event(HC_ACTION as i32, WM_KEYDOWN, VK_HANGUL.0 as u32, 0),
            Some(KeyboardEvent::HangulKeyPressed)
        );
    }

    #[test]
    fn synthetic_hangul_key_is_ignored() {
        assert_eq!(
            classify_event(
                HC_ACTION as i32,
                WM_KEYDOWN,
                VK_HANGUL.0 as u32,
                SYNTHETIC_INPUT_MARKER
            ),
            None
        );
    }
}
