//! Command-palette orchestration for exact thought transformations.

use crate::{
    application::{Action, Effect},
    ports::{
        editor::{CursorMovement, EditCommand},
        environment::{Clock, IdGenerator},
        text_layout::byte_for_position,
    },
};

use super::{BoardApp, palette::command::Command, palette_handoff::EditorSelectionHandoff};

impl BoardApp {
    pub(super) fn contextual_board_transformation(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.selection_len() >= 2 {
            let expected_sources: Vec<_> = self
                .action_thought_ids()
                .into_iter()
                .filter_map(|id| self.state.board.thought(id).cloned())
                .collect();
            return self.merge_selection(Some(&expected_sources), ids, clock);
        }
        let handoff = self.palette_selection_handoff.take();
        let Some(handoff) = handoff.as_ref() else {
            self.set_warning(
                "select contiguous thoughts, or press Esc then transform from an editor cursor",
            );
            return Vec::new();
        };
        if handoff.has_selection() {
            self.extract_handoff(Some(handoff), ids, clock)
        } else {
            self.split_at_handoff(Some(handoff), ids, clock)
        }
    }

    pub(super) fn contextual_edit_transformation(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.capture_palette_selection_handoff();
        let handoff = self.palette_selection_handoff.take();
        let mut effects = self.flush_pending_edit(ids, clock);
        let Some(handoff) = handoff.as_ref() else {
            self.set_warning("place the editor cursor or select text before transforming");
            return effects;
        };
        effects.extend(if handoff.has_selection() {
            self.extract_handoff(Some(handoff), ids, clock)
        } else {
            self.split_at_handoff(Some(handoff), ids, clock)
        });
        effects
    }

    pub(super) fn execute_transformation_command(
        &mut self,
        command: Command,
        handoff: Option<&EditorSelectionHandoff>,
        merge_handoff: Option<&[crate::domain::Thought]>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        match command {
            Command::SplitThought => Some(self.split_at_handoff(handoff, ids, clock)),
            Command::ExtractSelection => Some(self.extract_handoff(handoff, ids, clock)),
            Command::MergeThoughts => Some(self.merge_selection(merge_handoff, ids, clock)),
            _ => None,
        }
    }

    fn split_at_handoff(
        &mut self,
        handoff: Option<&EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(handoff) = handoff else {
            self.set_warning("place the editor cursor before opening commands");
            return Vec::new();
        };
        let Some(annotations) = self.exact_handoff_annotations(handoff) else {
            return Vec::new();
        };
        let at_byte = byte_for_position(&handoff.content, handoff.cursor);
        self.clear_expanded_folds(handoff.thought_id);
        let effects = self.reduce(Action::SplitThought {
            thought_id: handoff.thought_id,
            new_thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            expected_content: handoff.content.clone(),
            expected_annotations: annotations,
            at_byte,
            at: clock.now(),
        });
        if !effects.is_empty() {
            self.clear_board_selection();
            self.reload_editor();
            self.apply_edit(EditCommand::SetCursor {
                position: crate::domain::TextPosition::default(),
                extend_selection: false,
            });
        }
        effects
    }

    fn extract_handoff(
        &mut self,
        handoff: Option<&EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(handoff) = handoff else {
            self.set_warning("select exact editor text before opening commands");
            return Vec::new();
        };
        let Some(selection) = handoff.selection else {
            self.set_warning("select non-empty editor text before extracting it");
            return Vec::new();
        };
        let Some(annotations) = self.exact_handoff_annotations(handoff) else {
            return Vec::new();
        };
        let start = byte_for_position(&handoff.content, selection.start);
        let end = byte_for_position(&handoff.content, selection.end);
        self.clear_expanded_folds(handoff.thought_id);
        let effects = self.reduce(Action::ExtractThought {
            thought_id: handoff.thought_id,
            new_thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            expected_content: handoff.content.clone(),
            expected_annotations: annotations,
            range: start..end,
            at: clock.now(),
        });
        if !effects.is_empty() {
            self.clear_board_selection();
            self.reload_editor();
            self.apply_edit(EditCommand::Move {
                movement: CursorMovement::DocumentEnd,
                extend_selection: false,
            });
        }
        effects
    }

    fn merge_selection(
        &mut self,
        expected_sources: Option<&[crate::domain::Thought]>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.selection_len() < 2 {
            self.set_warning("select at least two contiguous thoughts before merging");
            return Vec::new();
        }
        let thought_ids = self.action_thought_ids();
        let Some(expected_sources) = expected_sources else {
            self.set_warning("selected thoughts changed after commands opened");
            return Vec::new();
        };
        let effects = self.reduce(Action::MergeThoughts {
            operation_id: ids.operation_id(),
            thought_ids,
            expected_sources: expected_sources.to_vec(),
            separator: self.settings.merge_separator.clone(),
            at: clock.now(),
        });
        if !effects.is_empty() {
            self.clear_board_selection();
            self.reload_editor();
        }
        effects
    }

    fn exact_handoff_annotations(
        &mut self,
        handoff: &EditorSelectionHandoff,
    ) -> Option<Vec<crate::domain::ContentAnnotation>> {
        let thought = self.state.board.thought(handoff.thought_id)?;
        if !thought.is_live()
            || thought.content != handoff.content
            || thought.annotations != handoff.annotations
        {
            self.set_warning("thought changed after the editor selection was captured");
            return None;
        }
        Some(handoff.annotations.clone())
    }
}
