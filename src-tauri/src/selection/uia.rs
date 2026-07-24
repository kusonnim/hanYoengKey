use windows::{
    core::Result as WindowsResult,
    Win32::{
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
        },
    },
};

use super::{
    provider::SelectionProvider,
    result::{SelectionError, SelectionResult},
};

pub(super) struct UiAutomationProvider;

impl SelectionProvider for UiAutomationProvider {
    fn get_selected_text(&self) -> SelectionResult {
        match selected_text() {
            Ok(Some(text)) if !text.is_empty() => SelectionResult::Success(text),
            Ok(_) => SelectionResult::NoSelection,
            Err(UiAutomationError::Unsupported) => SelectionResult::Unsupported,
            Err(UiAutomationError::Windows(error)) => {
                SelectionResult::Failure(SelectionError::UiAutomation(error))
            }
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
