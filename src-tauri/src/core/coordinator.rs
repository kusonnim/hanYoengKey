use std::sync::Mutex;

use crate::{
    converter::{self, ConversionDirection},
    input_language::{
        EnsureInputLanguageResult, InputLanguage, InputLanguageError, InputLanguageService,
    },
    replace::{ReplaceResult, ReplaceService},
    selection::{SelectionResult, SelectionService, SelectionSnapshot},
    settings::Settings,
};

use super::direction::choose_direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationState {
    Idle,
    ReadingSelection,
    ValidatingTarget,
    Converting,
    Replacing,
    RestoringClipboard,
    SynchronizingInputLanguage,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationError {
    NoSelection,
    UnsupportedTarget,
    TargetChanged,
    ClipboardBusy,
    ClipboardChangedExternally,
    SelectionReadTimeout,
    ReplacementTimeout,
    UIAutomationFailure,
    ReplacementFailure,
    ConversionFailure,
    OperationAlreadyInProgress,
    #[allow(dead_code)]
    InternalFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConversionOutcome {
    ConvertedAndSynchronized,
    ConvertedSynchronizationFailed(InputLanguageError),
    Failed(OperationError),
}

impl ConversionOutcome {
    pub(super) fn handled(self) -> bool {
        matches!(
            self,
            Self::ConvertedAndSynchronized | Self::ConvertedSynchronizationFailed(_)
        )
    }
}

pub(super) trait SelectionReader {
    fn selected_text(&self, settings: &Settings) -> SelectionResult;
}

pub(super) trait TextConverter {
    fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()>;
}

pub(super) trait SelectionReplacer {
    fn target_is_current(&self, selection: &SelectionSnapshot) -> bool;
    fn replace(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
        settings: &Settings,
    ) -> ReplaceResult;
}

pub(super) trait InputLanguageSynchronizer {
    fn ensure(
        &self,
        selection: &SelectionSnapshot,
        desired: InputLanguage,
    ) -> Result<EnsureInputLanguageResult, InputLanguageError>;
}

pub(super) struct ConversionCoordinator<S, C, R, L> {
    selection: S,
    converter: C,
    replacer: R,
    input_language: L,
    state: Mutex<OperationState>,
}

impl<S, C, R, L> ConversionCoordinator<S, C, R, L>
where
    S: SelectionReader,
    C: TextConverter,
    R: SelectionReplacer,
    L: InputLanguageSynchronizer,
{
    fn new(selection: S, converter: C, replacer: R, input_language: L) -> Self {
        Self {
            selection,
            converter,
            replacer,
            input_language,
            state: Mutex::new(OperationState::Idle),
        }
    }

    pub(super) fn process(&self, settings: &Settings) -> ConversionOutcome {
        {
            let mut state = self.state();
            if *state != OperationState::Idle {
                return ConversionOutcome::Failed(OperationError::OperationAlreadyInProgress);
            }
            *state = OperationState::ReadingSelection;
        }
        let _reset = StateReset(&self.state);

        let outcome = self.process_inner(settings);
        *self.state() = OperationState::Completed;
        if settings.debug_logging {
            eprintln!("{}", diagnostic_line(outcome));
        }
        outcome
    }

    fn process_inner(&self, settings: &Settings) -> ConversionOutcome {
        let selection = match self.selection.selected_text(settings) {
            SelectionResult::Success(snapshot) => snapshot,
            SelectionResult::NoSelection => return failed(OperationError::NoSelection),
            SelectionResult::Unsupported => return failed(OperationError::UnsupportedTarget),
            SelectionResult::TargetChanged => return failed(OperationError::TargetChanged),
            SelectionResult::TimedOut => return failed(OperationError::SelectionReadTimeout),
            SelectionResult::Failure(error) => {
                if settings.debug_logging {
                    eprintln!("[conversion] category=selection-provider-failure detail={error}");
                }
                return failed(OperationError::UIAutomationFailure);
            }
        };

        *self.state() = OperationState::Converting;
        let Some(direction) = choose_direction(&selection.text) else {
            return failed(OperationError::NoSelection);
        };
        let replacement = match self.converter.convert(&selection.text, direction) {
            Ok(replacement) => replacement,
            Err(()) => return failed(OperationError::ConversionFailure),
        };

        *self.state() = OperationState::ValidatingTarget;
        if !self.replacer.target_is_current(&selection) {
            return failed(OperationError::TargetChanged);
        }

        *self.state() = OperationState::Replacing;
        match self.replacer.replace(&selection, &replacement, settings) {
            ReplaceResult::Replaced => {}
            ReplaceResult::Unsupported => return failed(OperationError::UnsupportedTarget),
            ReplaceResult::TemporarilyUnavailable => return failed(OperationError::ClipboardBusy),
            ReplaceResult::TargetChanged => return failed(OperationError::TargetChanged),
            ReplaceResult::TimedOut => return failed(OperationError::ReplacementTimeout),
            ReplaceResult::ClipboardChangedExternally => {
                return failed(OperationError::ClipboardChangedExternally)
            }
            ReplaceResult::Failure(error) => {
                if settings.debug_logging {
                    eprintln!("[conversion] category=replacement-failure detail={error}");
                }
                return failed(OperationError::ReplacementFailure);
            }
        };
        *self.state() = OperationState::RestoringClipboard;

        *self.state() = OperationState::SynchronizingInputLanguage;
        let desired = match direction {
            ConversionDirection::EnglishToKorean => InputLanguage::Korean,
            ConversionDirection::KoreanToEnglish => InputLanguage::English,
        };
        match self.input_language.ensure(&selection, desired) {
            Ok(_) => ConversionOutcome::ConvertedAndSynchronized,
            Err(error) => ConversionOutcome::ConvertedSynchronizationFailed(error),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, OperationState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn failed(error: OperationError) -> ConversionOutcome {
    ConversionOutcome::Failed(error)
}

fn diagnostic_line(outcome: ConversionOutcome) -> String {
    format!("[conversion] outcome={outcome:?}")
}

struct StateReset<'a>(&'a Mutex<OperationState>);

impl Drop for StateReset<'_> {
    fn drop(&mut self) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = OperationState::Idle;
    }
}

pub(super) struct ConversionEngine;

impl TextConverter for ConversionEngine {
    fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()> {
        Ok(converter::convert(text, direction))
    }
}

impl SelectionReader for SelectionService {
    fn selected_text(&self, settings: &Settings) -> SelectionResult {
        self.get_selected_text(settings.selection_provider, settings.debug_logging)
    }
}

impl SelectionReplacer for ReplaceService {
    fn target_is_current(&self, selection: &SelectionSnapshot) -> bool {
        selection.target.is_current()
    }

    fn replace(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
        settings: &Settings,
    ) -> ReplaceResult {
        self.replace_selected_text(
            selection,
            replacement,
            settings.replacement_provider,
            settings.debug_logging,
        )
    }
}

impl InputLanguageSynchronizer for InputLanguageService {
    fn ensure(
        &self,
        selection: &SelectionSnapshot,
        desired: InputLanguage,
    ) -> Result<EnsureInputLanguageResult, InputLanguageError> {
        self.ensure_input_language(&selection.target, desired)
    }
}

pub(super) type ApplicationConversionCoordinator =
    ConversionCoordinator<SelectionService, ConversionEngine, ReplaceService, InputLanguageService>;

impl ApplicationConversionCoordinator {
    pub(super) fn application() -> Self {
        Self::new(
            SelectionService::new(),
            ConversionEngine,
            ReplaceService::new(),
            InputLanguageService::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::*;
    use crate::target::TargetIdentity;

    struct FakeSelection(SelectionResult);

    impl SelectionReader for FakeSelection {
        fn selected_text(&self, _settings: &Settings) -> SelectionResult {
            match &self.0 {
                SelectionResult::Success(snapshot) => SelectionResult::Success(snapshot.clone()),
                SelectionResult::NoSelection => SelectionResult::NoSelection,
                SelectionResult::Unsupported => SelectionResult::Unsupported,
                SelectionResult::TargetChanged => SelectionResult::TargetChanged,
                SelectionResult::TimedOut => SelectionResult::TimedOut,
                SelectionResult::Failure(_) => SelectionResult::TimedOut,
            }
        }
    }

    struct FakeConverter(bool);

    impl TextConverter for FakeConverter {
        fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()> {
            (!self.0)
                .then(|| converter::convert(text, direction))
                .ok_or(())
        }
    }

    struct FakeReplacer {
        calls: Arc<AtomicUsize>,
        result: ReplaceResult,
        target_current: bool,
    }

    struct FakeInputLanguage {
        calls: Arc<AtomicUsize>,
        requested: Arc<Mutex<Option<InputLanguage>>>,
        result: Result<EnsureInputLanguageResult, InputLanguageError>,
    }

    impl InputLanguageSynchronizer for FakeInputLanguage {
        fn ensure(
            &self,
            _selection: &SelectionSnapshot,
            desired: InputLanguage,
        ) -> Result<EnsureInputLanguageResult, InputLanguageError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            *self
                .requested
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(desired);
            self.result
        }
    }

    impl SelectionReplacer for FakeReplacer {
        fn target_is_current(&self, _selection: &SelectionSnapshot) -> bool {
            self.target_current
        }

        fn replace(
            &self,
            _selection: &SelectionSnapshot,
            _replacement: &str,
            _settings: &Settings,
        ) -> ReplaceResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            match self.result {
                ReplaceResult::Replaced => ReplaceResult::Replaced,
                ReplaceResult::Unsupported => ReplaceResult::Unsupported,
                ReplaceResult::TemporarilyUnavailable => ReplaceResult::TemporarilyUnavailable,
                ReplaceResult::TargetChanged => ReplaceResult::TargetChanged,
                ReplaceResult::TimedOut => ReplaceResult::TimedOut,
                ReplaceResult::ClipboardChangedExternally => {
                    ReplaceResult::ClipboardChangedExternally
                }
                ReplaceResult::Failure(_) => ReplaceResult::TemporarilyUnavailable,
            }
        }
    }

    fn snapshot(text: &str) -> SelectionSnapshot {
        SelectionSnapshot {
            text: text.into(),
            target: TargetIdentity::test_identity(),
        }
    }

    fn coordinator(
        selection: SelectionResult,
        converter_fails: bool,
        replacement: ReplaceResult,
        target_current: bool,
    ) -> (
        ConversionCoordinator<FakeSelection, FakeConverter, FakeReplacer, FakeInputLanguage>,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
        Arc<Mutex<Option<InputLanguage>>>,
    ) {
        coordinator_with_language_result(
            selection,
            converter_fails,
            replacement,
            target_current,
            Ok(EnsureInputLanguageResult::Changed),
        )
    }

    fn coordinator_with_language_result(
        selection: SelectionResult,
        converter_fails: bool,
        replacement: ReplaceResult,
        target_current: bool,
        language_result: Result<EnsureInputLanguageResult, InputLanguageError>,
    ) -> (
        ConversionCoordinator<FakeSelection, FakeConverter, FakeReplacer, FakeInputLanguage>,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
        Arc<Mutex<Option<InputLanguage>>>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let language_calls = Arc::new(AtomicUsize::new(0));
        let requested = Arc::new(Mutex::new(None));
        (
            ConversionCoordinator::new(
                FakeSelection(selection),
                FakeConverter(converter_fails),
                FakeReplacer {
                    calls: Arc::clone(&calls),
                    result: replacement,
                    target_current,
                },
                FakeInputLanguage {
                    calls: Arc::clone(&language_calls),
                    requested: Arc::clone(&requested),
                    result: language_result,
                },
            ),
            calls,
            language_calls,
            requested,
        )
    }

    #[test]
    fn no_selection_does_not_replace_and_returns_to_idle() {
        let (coordinator, calls, language_calls, _) = coordinator(
            SelectionResult::NoSelection,
            false,
            ReplaceResult::Replaced,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::NoSelection)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(language_calls.load(Ordering::Relaxed), 0);
        assert_eq!(*coordinator.state(), OperationState::Idle);
    }

    #[test]
    fn changed_target_does_not_replace() {
        let (coordinator, calls, language_calls, _) = coordinator(
            SelectionResult::Success(snapshot("hello")),
            false,
            ReplaceResult::Replaced,
            false,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::TargetChanged)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(language_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn changed_selection_is_reported_by_replacer() {
        let (coordinator, calls, language_calls, _) = coordinator(
            SelectionResult::Success(snapshot("hello")),
            false,
            ReplaceResult::TargetChanged,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::TargetChanged)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(language_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn conversion_failure_never_replaces() {
        let (coordinator, calls, language_calls, _) = coordinator(
            SelectionResult::Success(snapshot("hello")),
            true,
            ReplaceResult::Replaced,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::ConversionFailure)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(language_calls.load(Ordering::Relaxed), 0);
        assert_eq!(*coordinator.state(), OperationState::Idle);
    }

    #[test]
    fn successful_conversion_replaces_once_and_returns_to_idle() {
        let (coordinator, calls, language_calls, requested) = coordinator(
            SelectionResult::Success(snapshot("dkssud")),
            false,
            ReplaceResult::Replaced,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            ConversionOutcome::ConvertedAndSynchronized
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(language_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            *requested
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            Some(InputLanguage::Korean)
        );
        assert_eq!(*coordinator.state(), OperationState::Idle);
    }

    #[test]
    fn korean_to_english_requests_english_input_mode() {
        let (coordinator, _, language_calls, requested) = coordinator(
            SelectionResult::Success(snapshot("안녕")),
            false,
            ReplaceResult::Replaced,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            ConversionOutcome::ConvertedAndSynchronized
        );
        assert_eq!(language_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            *requested
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            Some(InputLanguage::English)
        );
    }

    #[test]
    fn synchronization_failure_is_partial_success_after_one_replacement() {
        let (coordinator, replacement_calls, language_calls, _) = coordinator_with_language_result(
            SelectionResult::Success(snapshot("dkssud")),
            false,
            ReplaceResult::Replaced,
            true,
            Err(InputLanguageError::TargetChanged),
        );
        let outcome = coordinator.process(&Settings::default());
        assert_eq!(
            outcome,
            ConversionOutcome::ConvertedSynchronizationFailed(InputLanguageError::TargetChanged)
        );
        assert!(outcome.handled());
        assert_eq!(replacement_calls.load(Ordering::Relaxed), 1);
        assert_eq!(language_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn concurrent_activation_is_rejected() {
        let (coordinator, _, _, _) = coordinator(
            SelectionResult::NoSelection,
            false,
            ReplaceResult::Replaced,
            true,
        );
        *coordinator.state() = OperationState::ReadingSelection;
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::OperationAlreadyInProgress)
        );
    }

    #[test]
    fn timeout_categories_are_distinct() {
        let (replacement_timeout, _, _, _) = coordinator(
            SelectionResult::Success(snapshot("hello")),
            false,
            ReplaceResult::TimedOut,
            true,
        );
        assert_eq!(
            replacement_timeout.process(&Settings::default()),
            failed(OperationError::ReplacementTimeout)
        );
        let (coordinator, _, _, _) = coordinator(
            SelectionResult::TimedOut,
            false,
            ReplaceResult::Replaced,
            true,
        );
        assert_eq!(
            coordinator.process(&Settings::default()),
            failed(OperationError::SelectionReadTimeout)
        );
    }

    #[test]
    fn diagnostics_never_include_selected_text() {
        let sensitive = "private selected text";
        let line = diagnostic_line(failed(OperationError::TargetChanged));
        assert!(!line.contains(sensitive));
        assert_eq!(line, "[conversion] outcome=Failed(TargetChanged)");
    }
}
