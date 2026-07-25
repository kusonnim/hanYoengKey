use std::{sync::mpsc, thread, time::Duration};

use windows::{
    core::BSTR,
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
            CUIAutomation8, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
            UIA_TextPatternId, UIA_ValuePatternId,
        },
    },
};

use crate::selection::SelectionSnapshot;

use super::{
    provider::ReplaceProvider,
    result::{ReplaceError, ReplaceResult},
};

pub(super) struct UiAutomationProvider;
const UIA_TIMEOUT: Duration = Duration::from_millis(300);

impl ReplaceProvider for UiAutomationProvider {
    fn replace_selected_text(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
    ) -> ReplaceResult {
        let expected = selection.text.clone();
        let replacement = replacement.to_owned();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (thread_sender, thread_receiver) = mpsc::sync_channel(1);
        if thread::Builder::new()
            .name("uia-replacement".into())
            .spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                let _ = thread_sender.send(thread_id);
                let result = unsafe {
                    let _ = CoEnableCallCancellation(None);
                    match OleInitialize(None) {
                        Ok(()) => {
                            let result = replace_full_control_selection(&expected, &replacement);
                            OleUninitialize();
                            result
                        }
                        Err(error) => Err(error),
                    }
                };
                unsafe {
                    let _ = CoDisableCallCancellation(None);
                }
                let _ = sender.send(result);
            })
            .is_err()
        {
            return ReplaceResult::Failure(ReplaceError::Clipboard(
                "could not start UI Automation worker".into(),
            ));
        }

        let thread_id = thread_receiver.recv_timeout(Duration::from_millis(50)).ok();
        match receiver.recv_timeout(UIA_TIMEOUT) {
            Ok(Ok(true)) => ReplaceResult::Replaced,
            Ok(Ok(false)) => ReplaceResult::Unsupported,
            Ok(Err(error)) => ReplaceResult::Failure(ReplaceError::UiAutomation(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(thread_id) = thread_id {
                    unsafe {
                        let _ = CoCancelCall(thread_id, 0);
                    }
                }
                ReplaceResult::TimedOut
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => ReplaceResult::Failure(
                ReplaceError::Clipboard("UI Automation worker ended unexpectedly".into()),
            ),
        }
    }
}

fn replace_full_control_selection(
    expected: &str,
    replacement: &str,
) -> windows::core::Result<bool> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
    let focused = unsafe { automation.GetFocusedElement()? };

    let text_pattern: IUIAutomationTextPattern =
        match unsafe { focused.GetCurrentPatternAs(UIA_TextPatternId) } {
            Ok(pattern) => pattern,
            Err(_) => return Ok(false),
        };
    let value_pattern: IUIAutomationValuePattern =
        match unsafe { focused.GetCurrentPatternAs(UIA_ValuePatternId) } {
            Ok(pattern) => pattern,
            Err(_) => return Ok(false),
        };
    if unsafe { value_pattern.CurrentIsReadOnly()?.as_bool() } {
        return Ok(false);
    }

    let ranges = unsafe { text_pattern.GetSelection()? };
    if unsafe { ranges.Length()? } != 1 {
        return Ok(false);
    }
    let selection = unsafe { ranges.GetElement(0)?.GetText(-1)? }.to_string();
    if selection.is_empty() || selection != expected {
        return Ok(false);
    }

    let document = unsafe { text_pattern.DocumentRange()?.GetText(-1)? }.to_string();
    let current_value = unsafe { value_pattern.CurrentValue()? }.to_string();
    if selection != document || document != current_value {
        // ValuePattern writes the whole control. It is safe only when the user
        // selected the entire current value; partial selections use fallback.
        return Ok(false);
    }

    unsafe {
        value_pattern.SetValue(&BSTR::from(replacement))?;
    }
    Ok(true)
}
