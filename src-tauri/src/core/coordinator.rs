use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    converter::{self, ConversionDirection},
    replace::{ReplaceResult, ReplaceService},
    selection::{SelectionResult, SelectionService},
};

use super::direction::choose_direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConversionOutcome {
    Converted,
    NoSelection,
    Unsupported,
    TemporarilyUnavailable,
    ReplacementFailed,
    InternalFailure,
}

pub(super) trait SelectionReader {
    fn selected_text(&self) -> SelectionResult;
}

pub(super) trait TextConverter {
    fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()>;
}

pub(super) trait SelectionReplacer {
    fn replace(&self, replacement: &str) -> ReplaceResult;
}

pub(super) struct ConversionCoordinator<S, C, R> {
    selection: S,
    converter: C,
    replacer: R,
    in_progress: AtomicBool,
}

impl<S, C, R> ConversionCoordinator<S, C, R>
where
    S: SelectionReader,
    C: TextConverter,
    R: SelectionReplacer,
{
    fn new(selection: S, converter: C, replacer: R) -> Self {
        Self {
            selection,
            converter,
            replacer,
            in_progress: AtomicBool::new(false),
        }
    }

    pub(super) fn process(&self) -> ConversionOutcome {
        if self.in_progress.swap(true, Ordering::AcqRel) {
            return ConversionOutcome::TemporarilyUnavailable;
        }
        let _guard = ProgressGuard(&self.in_progress);
        self.process_inner()
    }

    fn process_inner(&self) -> ConversionOutcome {
        let selected = match self.selection.selected_text() {
            SelectionResult::Success(text) => text,
            SelectionResult::NoSelection => return ConversionOutcome::NoSelection,
            SelectionResult::Unsupported => return ConversionOutcome::Unsupported,
            SelectionResult::Failure(_) => return ConversionOutcome::TemporarilyUnavailable,
        };

        let Some(direction) = choose_direction(&selected) else {
            return ConversionOutcome::NoSelection;
        };
        let replacement = match self.converter.convert(&selected, direction) {
            Ok(replacement) => replacement,
            Err(()) => return ConversionOutcome::InternalFailure,
        };

        match self.replacer.replace(&replacement) {
            ReplaceResult::Replaced => ConversionOutcome::Converted,
            ReplaceResult::Unsupported => ConversionOutcome::Unsupported,
            ReplaceResult::TemporarilyUnavailable => ConversionOutcome::TemporarilyUnavailable,
            ReplaceResult::Failure(_) => ConversionOutcome::ReplacementFailed,
        }
    }
}

struct ProgressGuard<'a>(&'a AtomicBool);

impl Drop for ProgressGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct ConversionEngine;

impl TextConverter for ConversionEngine {
    fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()> {
        Ok(converter::convert(text, direction))
    }
}

impl SelectionReader for SelectionService {
    fn selected_text(&self) -> SelectionResult {
        self.get_selected_text()
    }
}

impl SelectionReplacer for ReplaceService {
    fn replace(&self, replacement: &str) -> ReplaceResult {
        self.replace_selected_text(replacement)
    }
}

pub(super) type ApplicationConversionCoordinator =
    ConversionCoordinator<SelectionService, ConversionEngine, ReplaceService>;

impl ApplicationConversionCoordinator {
    pub(super) fn application() -> Self {
        Self::new(
            SelectionService::new(),
            ConversionEngine,
            ReplaceService::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::{replace::ReplaceError, selection::SelectionError};

    use super::*;

    struct FakeSelection(SelectionResult);

    impl SelectionReader for FakeSelection {
        fn selected_text(&self) -> SelectionResult {
            match &self.0 {
                SelectionResult::Success(text) => SelectionResult::Success(text.clone()),
                SelectionResult::NoSelection => SelectionResult::NoSelection,
                SelectionResult::Unsupported => SelectionResult::Unsupported,
                SelectionResult::Failure(_) => {
                    SelectionResult::Failure(SelectionError::Clipboard("fake failure".into()))
                }
            }
        }
    }

    struct FakeConverter {
        fail: bool,
    }

    impl TextConverter for FakeConverter {
        fn convert(&self, text: &str, direction: ConversionDirection) -> Result<String, ()> {
            (!self.fail)
                .then(|| converter::convert(text, direction))
                .ok_or(())
        }
    }

    struct FakeReplacer {
        calls: Arc<AtomicUsize>,
        result: ReplaceResult,
    }

    impl SelectionReplacer for FakeReplacer {
        fn replace(&self, _replacement: &str) -> ReplaceResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            match self.result {
                ReplaceResult::Replaced => ReplaceResult::Replaced,
                ReplaceResult::Unsupported => ReplaceResult::Unsupported,
                ReplaceResult::TemporarilyUnavailable => ReplaceResult::TemporarilyUnavailable,
                ReplaceResult::Failure(_) => {
                    ReplaceResult::Failure(ReplaceError::PasteInputBlocked)
                }
            }
        }
    }

    fn coordinator(
        selection: SelectionResult,
        converter_fails: bool,
        replacement: ReplaceResult,
    ) -> (
        ConversionCoordinator<FakeSelection, FakeConverter, FakeReplacer>,
        Arc<AtomicUsize>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            ConversionCoordinator::new(
                FakeSelection(selection),
                FakeConverter {
                    fail: converter_fails,
                },
                FakeReplacer {
                    calls: Arc::clone(&calls),
                    result: replacement,
                },
            ),
            calls,
        )
    }

    #[test]
    fn no_selection_does_not_replace() {
        let (coordinator, calls) =
            coordinator(SelectionResult::NoSelection, false, ReplaceResult::Replaced);
        assert_eq!(coordinator.process(), ConversionOutcome::NoSelection);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn conversion_failure_does_not_replace() {
        let (coordinator, calls) = coordinator(
            SelectionResult::Success("hello".into()),
            true,
            ReplaceResult::Replaced,
        );
        assert_eq!(coordinator.process(), ConversionOutcome::InternalFailure);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn successful_conversion_replaces_exactly_once() {
        let (coordinator, calls) = coordinator(
            SelectionResult::Success("dkssud".into()),
            false,
            ReplaceResult::Replaced,
        );
        assert_eq!(coordinator.process(), ConversionOutcome::Converted);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn replacement_failure_is_structured() {
        let (coordinator, calls) = coordinator(
            SelectionResult::Success("hello".into()),
            false,
            ReplaceResult::Failure(ReplaceError::PasteInputBlocked),
        );
        assert_eq!(coordinator.process(), ConversionOutcome::ReplacementFailed);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn reentrant_activation_is_rejected() {
        let (coordinator, calls) =
            coordinator(SelectionResult::NoSelection, false, ReplaceResult::Replaced);
        coordinator.in_progress.store(true, Ordering::Release);
        assert_eq!(
            coordinator.process(),
            ConversionOutcome::TemporarilyUnavailable
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
}
