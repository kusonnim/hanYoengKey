use std::{sync::mpsc, thread, time::Duration};

use windows::{
    core::Result as WindowsResult,
    Win32::{
        System::{
            Com::{
                CoCancelCall, CoCreateInstance, CoDisableCallCancellation,
                CoEnableCallCancellation, CLSCTX_INPROC_SERVER,
            },
            Ole::{OleInitialize, OleUninitialize},
            Threading::GetCurrentThreadId,
        },
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
        },
    },
};

use super::{
    provider::SelectionProvider,
    result::{SelectionError, SelectionResult, SelectionSnapshot},
};

pub(super) struct UiAutomationProvider;
const UIA_TIMEOUT: Duration = Duration::from_millis(250);

impl SelectionProvider for UiAutomationProvider {
    fn get_selected_text(&self) -> SelectionResult {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (thread_sender, thread_receiver) = mpsc::sync_channel(1);
        if thread::Builder::new()
            .name("uia-selection".into())
            .spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                let _ = thread_sender.send(thread_id);
                let result = unsafe {
                    let _ = CoEnableCallCancellation(None);
                    match OleInitialize(None) {
                        Ok(()) => {
                            let result = selected_text();
                            OleUninitialize();
                            result
                        }
                        Err(error) => Err(UiAutomationError::Windows(error)),
                    }
                };
                unsafe {
                    let _ = CoDisableCallCancellation(None);
                }
                let _ = sender.send(result);
            })
            .is_err()
        {
            return SelectionResult::Failure(SelectionError::Clipboard(
                "could not start UI Automation worker".into(),
            ));
        }

        let thread_id = thread_receiver.recv_timeout(Duration::from_millis(50)).ok();
        match receiver.recv_timeout(UIA_TIMEOUT) {
            Ok(Ok(Some(text))) if !text.is_empty() => SelectionSnapshot::capture(text)
                .map(SelectionResult::Success)
                .unwrap_or(SelectionResult::TargetChanged),
            Ok(Ok(_)) => SelectionResult::NoSelection,
            Ok(Err(UiAutomationError::Unsupported)) => SelectionResult::Unsupported,
            Ok(Err(UiAutomationError::Windows(error))) => {
                SelectionResult::Failure(SelectionError::UiAutomation(error))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(thread_id) = thread_id {
                    unsafe {
                        let _ = CoCancelCall(thread_id, 0);
                    }
                }
                SelectionResult::TimedOut
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => SelectionResult::Failure(
                SelectionError::Clipboard("UI Automation worker ended unexpectedly".into()),
            ),
        }
    }
}

enum UiAutomationError {
    Unsupported,
    Windows(windows::core::Error),
}

impl From<windows::core::Error> for UiAutomationError {
    fn from(error: windows::core::Error) -> Self {
        Self::Windows(error)
    }
}

fn selected_text() -> Result<Option<String>, UiAutomationError> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
    let focused = unsafe { automation.GetFocusedElement()? };

    let pattern: IUIAutomationTextPattern =
        match unsafe { focused.GetCurrentPatternAs(UIA_TextPatternId) } {
            Ok(pattern) => pattern,
            Err(_) => return Err(UiAutomationError::Unsupported),
        };

    collect_selection(&pattern).map_err(UiAutomationError::Windows)
}

fn collect_selection(pattern: &IUIAutomationTextPattern) -> WindowsResult<Option<String>> {
    let ranges = unsafe { pattern.GetSelection()? };
    let range_count = unsafe { ranges.Length()? };
    let mut selected = String::new();

    for index in 0..range_count {
        let range = unsafe { ranges.GetElement(index)? };
        let text = unsafe { range.GetText(-1)? };
        selected.push_str(&text.to_string());
    }

    Ok((!selected.is_empty()).then_some(selected))
}
