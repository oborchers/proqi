//! Semantic editor coalescing before durable reducer revisions.

use crate::{
    application::{Action, Effect, reduce},
    domain::ThoughtId,
    ports::{
        editor::{EditCommand, EditorSnapshot},
        environment::{Clock, IdGenerator},
    },
};

use super::BoardApp;

pub(super) struct PendingEdit {
    thought_id: ThoughtId,
    before: EditorSnapshot,
    after: EditorSnapshot,
}

impl BoardApp {
    pub(super) fn apply_edit(&mut self, command: EditCommand) {
        let edit = self.editor.as_mut().and_then(|(thought_id, editor)| {
            let before = editor.snapshot();
            let outcome = editor.apply(command);
            outcome
                .content_changed
                .then_some((*thought_id, before, outcome.snapshot))
        });
        let Some((thought_id, before, after)) = edit else {
            return;
        };
        if self.is_draft(thought_id) {
            self.edit_generation = self.edit_generation.wrapping_add(1);
            return;
        }
        match &mut self.pending_edit {
            Some(pending) if pending.thought_id == thought_id => pending.after = after,
            _ => {
                self.pending_edit = Some(PendingEdit {
                    thought_id,
                    before,
                    after,
                });
            }
        }
        self.edit_generation = self.edit_generation.wrapping_add(1);
    }

    /// Commit accumulated typing as one persistent editor revision.
    pub fn flush_pending_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(pending) = self.pending_edit.as_ref() else {
            return Vec::new();
        };
        let action = Action::EditThought {
            thought_id: pending.thought_id,
            revision_id: ids.revision_id(),
            before_content: pending.before.content.clone(),
            after_content: pending.after.content.clone(),
            before_cursor: pending.before.cursor,
            after_cursor: pending.after.cursor,
            at: clock.now(),
        };
        match reduce(&mut self.state, action) {
            Ok(effects) => {
                self.pending_edit = None;
                effects
            }
            Err(error) => {
                self.status = Some(error.to_string());
                Vec::new()
            }
        }
    }

    pub(super) fn pending_edit_snapshot(&self) -> Option<(ThoughtId, &EditorSnapshot)> {
        self.pending_edit
            .as_ref()
            .map(|pending| (pending.thought_id, &pending.after))
    }
}
