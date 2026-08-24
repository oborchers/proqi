//! One empty UI-owned draft that does not enter durable history prematurely.

use crate::{
    application::{Action, AppState, Effect, InteractionMode},
    domain::{SessionBoard, Thought, ThoughtId, ThoughtPosition, Timestamp},
    ports::environment::{Clock, IdGenerator},
};

use super::BoardApp;

pub(super) struct DraftState {
    pub(super) thought_id: ThoughtId,
    insertion_index: usize,
    created_at: Timestamp,
}

impl BoardApp {
    pub(super) fn start_draft(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.draft.is_some() {
            return Vec::new();
        }
        let thought_id = ids.thought_id();
        let mut editor = self.editor_factory.create("");
        editor.set_viewport(self.viewport);
        self.editor = Some((thought_id, editor));
        self.draft = Some(DraftState {
            thought_id,
            insertion_index: self.state.insertion_index,
            created_at: clock.now(),
        });
        self.manual_board_scroll = false;
        self.layout = None;
        Vec::new()
    }

    pub(super) fn persist_draft(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(draft) = self.draft.as_ref() else {
            return Vec::new();
        };
        let content = self
            .editor_snapshot()
            .map_or_else(String::new, |snapshot| snapshot.content);
        if content.is_empty() {
            return Vec::new();
        }
        let thought_id = draft.thought_id;
        let insertion_index = draft.insertion_index;
        let effects = self.reduce(Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content,
            insertion_index: Some(insertion_index),
            at: clock.now(),
        });
        if self.state.board.thought(thought_id).is_some() {
            self.draft = None;
            self.sync_editor_from_state();
        }
        effects
    }

    pub(super) fn discard_draft(&mut self) {
        if self.draft.take().is_some() {
            self.editor = None;
            self.layout = None;
        }
    }

    pub(super) fn is_draft(&self, thought_id: ThoughtId) -> bool {
        self.draft
            .as_ref()
            .is_some_and(|draft| draft.thought_id == thought_id)
    }

    pub(in crate::ui) fn content_for_render(&self, thought_id: ThoughtId) -> Option<String> {
        if self.is_draft(thought_id) {
            return self.editor_snapshot().map(|snapshot| snapshot.content);
        }
        self.state
            .board
            .thought(thought_id)
            .map(|thought| thought.content.clone())
    }

    pub(super) fn layout_state_with_draft(&self) -> AppState {
        let Some(draft) = &self.draft else {
            return self.state.clone();
        };
        let mut thoughts = self.state.board.thoughts().to_vec();
        for thought in &mut thoughts {
            if thought.is_live()
                && usize::try_from(thought.position.get()).unwrap_or(usize::MAX)
                    >= draft.insertion_index
            {
                thought.position = ThoughtPosition::new(thought.position.get().saturating_add(1));
            }
        }
        thoughts.push(Thought::new(
            draft.thought_id,
            self.state.board.session.id,
            self.editor_snapshot()
                .map_or_else(String::new, |snapshot| snapshot.content),
            ThoughtPosition::new(u32::try_from(draft.insertion_index).unwrap_or(u32::MAX)),
            draft.created_at,
        ));
        let Ok(board) = SessionBoard::new(self.state.board.session.clone(), thoughts) else {
            return self.state.clone();
        };
        let mut state = self.state.clone();
        state.board = board;
        state.focused_thought = Some(draft.thought_id);
        state.mode = InteractionMode::Edit {
            thought_id: draft.thought_id,
        };
        state
    }
}
