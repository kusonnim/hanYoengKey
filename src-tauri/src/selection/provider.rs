use super::SelectionResult;

pub(super) trait SelectionProvider {
    fn get_selected_text(&self) -> SelectionResult;
}
