//! Semantic editor coalescing before durable reducer revisions.

use crate::{
    application::{Action, Effect, reduce},
    domain::{ContentAnnotation, ThoughtId},
    ports::{
        editor::{EditCommand, EditorSnapshot},
        environment::{Clock, IdGenerator},
    },
};

use super::BoardApp;
use crate::ui::annotations;

pub(super) struct PendingEdit {
    thought_id: ThoughtId,
    before: EditorSnapshot,
    after: EditorSnapshot,
    before_annotations: Vec<ContentAnnotation>,
    after_annotations: Vec<ContentAnnotation>,
}

impl BoardApp {
    pub(super) fn current_annotations(&self, thought_id: ThoughtId) -> Vec<ContentAnnotation> {
        self.draft_annotations(thought_id).unwrap_or_else(|| {
            self.pending_edit
                .as_ref()
                .filter(|pending| pending.thought_id == thought_id)
                .map(|pending| pending.after_annotations.clone())
                .or_else(|| {
                    self.state
                        .board
                        .thought(thought_id)
                        .map(|thought| thought.annotations.clone())
                })
                .unwrap_or_default()
        })
    }

    pub(super) fn apply_edit(&mut self, command: EditCommand) {
        self.apply_annotated_edit(command, &[]);
    }

    pub(super) fn apply_annotated_edit(
        &mut self,
        command: EditCommand,
        inserted_annotations: &[ContentAnnotation],
    ) {
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
        self.clear_expanded_folds(thought_id);
        if self.is_draft(thought_id) {
            self.edit_generation = self.edit_generation.wrapping_add(1);
            return;
        }
        let current_annotations = self
            .pending_edit
            .as_ref()
            .filter(|pending| pending.thought_id == thought_id)
            .map_or_else(
                || {
                    self.state
                        .board
                        .thought(thought_id)
                        .map_or_else(Vec::new, |thought| thought.annotations.clone())
                },
                |pending| pending.after_annotations.clone(),
            );
        let after_annotations = annotations::rebase(
            &before.content,
            &after.content,
            &current_annotations,
            inserted_annotations,
        );
        match &mut self.pending_edit {
            Some(pending) if pending.thought_id == thought_id => {
                pending.after = after;
                pending.after_annotations = after_annotations;
            }
            _ => {
                self.pending_edit = Some(PendingEdit {
                    thought_id,
                    before,
                    after,
                    before_annotations: current_annotations,
                    after_annotations,
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
            before_annotations: pending.before_annotations.clone(),
            after_annotations: pending.after_annotations.clone(),
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
