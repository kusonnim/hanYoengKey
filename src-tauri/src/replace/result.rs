use thiserror::Error;

#[derive(Debug)]
pub(crate) enum ReplaceResult {
    Replaced,
    Unsupported,
    TemporarilyUnavailable,
    TargetChanged,
    TimedOut,
    ClipboardChangedExternally,
    Failure(ReplaceError),
}

#[derive(Debug, Error)]
pub(crate) enum ReplaceError {
    #[error("COM initialization failed: {0}")]
    Com(#[source] windows::core::Error),
    #[error("UI Automation replacement failed: {0}")]
    UiAutomation(#[source] windows::core::Error),
    #[error("clipboard replacement failed: {0}")]
    Clipboard(String),
    #[error("clipboard restoration failed: {0}")]
    ClipboardRestore(#[source] windows::core::Error),
    #[error(
        "both replacement providers failed (UI Automation: {preferred}; clipboard: {fallback})"
    )]
    ProvidersFailed { preferred: String, fallback: String },
}
