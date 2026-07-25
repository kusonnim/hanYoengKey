use windows::{
    core::BSTR,
    Win32::{
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
            UIA_TextPatternId, UIA_ValuePatternId,
        },
    },
};

use super::{
    provider::ReplaceProvider,
    result::{ReplaceError, ReplaceResult},
};

pub(super) struct UiAutomationProvider;

impl ReplaceProvider for UiAutomationProvider {
    fn replace_selected_text(&self, replacement: &str) -> ReplaceResult {
        match replace_full_control_selection(replacement) {
            Ok(true) => ReplaceResult::Replaced,
            Ok(false) => ReplaceResult::Unsupported,
            Err(error) => ReplaceResult::Failure(ReplaceError::UiAutomation(error)),
        }
    }
}

fn replace_full_control_selection(replacement: &str) -> windows::core::Result<bool> {
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
    if selection.is_empty() {
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
