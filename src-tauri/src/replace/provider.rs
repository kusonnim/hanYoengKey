use crate::selection::SelectionSnapshot;

use super::ReplaceResult;

pub(super) trait ReplaceProvider {
    fn replace_selected_text(
        &self,
        selection: &SelectionSnapshot,
        replacement: &str,
    ) -> ReplaceResult;
}
