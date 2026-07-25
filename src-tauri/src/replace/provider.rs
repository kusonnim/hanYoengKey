use super::ReplaceResult;

pub(super) trait ReplaceProvider {
    fn replace_selected_text(&self, replacement: &str) -> ReplaceResult;
}
