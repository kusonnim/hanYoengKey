use thiserror::Error;

#[derive(Debug)]
pub(crate) enum SelectionResult {
    Success(String),
    NoSelection,
    Unsupported,
    Failure(SelectionError),
}

#[derive(Debug, Error)]
pub(crate) enum SelectionError {
    #[error("COM initialization failed: {0}")]
    Com(#[source] windows::core::Error),
    #[error("UI Automation failed: {0}")]
    UiAutomation(#[source] windows::core::Error),
    #[error("clipboard operation failed: {0}")]
    Clipboard(String),
    #[error("clipboard restoration failed: {0}")]
    ClipboardRestore(#[source] windows::core::Error),
    #[error("both selection providers failed (UI Automation: {preferred}; clipboard: {fallback})")]
    ProvidersFailed { preferred: String, fallback: String },
}
