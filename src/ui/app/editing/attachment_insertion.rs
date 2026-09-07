//! Allocate pasted occurrences before mutating an editor buffer.

use crate::{domain::ContentAnnotation, ui::BoardApp};

impl BoardApp {
    pub(in crate::ui::app) fn prepare_attachment_insertion(
        &mut self,
        inserted: &[ContentAnnotation],
    ) -> Option<(Vec<ContentAnnotation>, Vec<ContentAnnotation>)> {
        let thought = self.active_thought_id()?;
        let current = self.current_annotations(thought);
        let mut inserted = inserted.to_vec();
        crate::domain::renew_attachment_occurrences(&mut inserted);
        let mut counters = self.state.board.attachment_counters();
        if counters
            .observe(&current)
            .and_then(|()| counters.assign(&mut inserted))
            .is_err()
        {
            self.set_error("attachment numbering exhausted");
            return None;
        }
        Some((current, inserted))
    }
}
