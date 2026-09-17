//! Canonical new-thought policy and generic thought-creation mechanics.

use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{ContentAnnotation, OperationId, TextPosition, ThoughtId, Timestamp},
    ports::environment::{Clock, IdGenerator},
    ui::PastePayload,
};

use super::{BoardApp, ComposePresentation, InsertionConfirmation, InsertionFocus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NewThoughtPlacement {
    Contextual,
    DurableTail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CreationHistory {
    Board {
        insertion_index: Option<usize>,
    },
    Compose {
        cursor: TextPosition,
        selection_anchor: Option<TextPosition>,
    },
}

impl BoardApp {
    /// Insert a payload-free durable separator below the focused item.
    pub(super) fn insert_separator(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let insertion_index = self
            .state
            .focused_item
            .and_then(|id| self.state.board.item_position(id))
            .and_then(|position| usize::try_from(position.get()).ok())
            .map_or_else(
                || {
                    self.state
                        .insertion_index
                        .min(self.state.board.live_items().len())
                },
                |position| position.saturating_add(1),
            );
        self.clear_board_selection();
        self.compose_presentation = ComposePresentation::Prompt;
        self.insertion_focus = InsertionFocus::Inactive;
        self.insertion_confirmation = InsertionConfirmation::Idle;
        let effects = self.reduce(Action::InsertSeparator {
            separator_id: ids.separator_id(),
            operation_id: ids.operation_id(),
            insertion_index,
            at: clock.now(),
        });
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
        self.sync_editor_from_state();
        effects
    }

    /// Route every deliberate New-thought intention through one lifecycle owner.
    pub(super) fn new_thought(
        &mut self,
        placement: NewThoughtPlacement,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Compose)
            || self.state.board.live_items().is_empty()
        {
            self.enter_provisional_compose()
        } else if let Some(crate::domain::BoardItemId::Separator(separator_id)) =
            self.state.focused_item
        {
            let insertion = self
                .state
                .board
                .separator(separator_id)
                .and_then(|separator| usize::try_from(separator.position.get()).ok())
                .map(|position| position.saturating_add(1));
            match insertion {
                Some(index) => self.create_blank_at(index, ids, clock),
                None => self.create_blank(ids, clock),
            }
        } else if placement == NewThoughtPlacement::DurableTail || self.insertion_focused() {
            self.create_blank_at_bottom(ids, clock)
        } else {
            self.create_blank(ids, clock)
        }
    }

    pub(super) fn enter_provisional_compose(&mut self) -> Vec<Effect> {
        let effects = self.reduce(Action::EnterCompose);
        self.sync_editor_from_state();
        self.compose_presentation = ComposePresentation::Editor;
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
        effects
    }

    pub(super) fn create_blank(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_blank_with_insertion_index(None, ids, clock)
    }

    pub(super) fn create_blank_at(
        &mut self,
        insertion_index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_blank_with_insertion_index(Some(insertion_index), ids, clock)
    }

    fn create_blank_with_insertion_index(
        &mut self,
        insertion_index: Option<usize>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_with_insertion_index(
            PastePayload::text(String::new()),
            insertion_index,
            ids,
            clock,
        )
    }

    pub(super) fn create(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_with_insertion_index(payload, None, ids, clock)
    }

    pub(super) fn create_at(
        &mut self,
        payload: PastePayload,
        insertion_index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_with_insertion_index(payload, Some(insertion_index), ids, clock)
    }

    fn create_with_insertion_index(
        &mut self,
        payload: PastePayload,
        insertion_index: Option<usize>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.clear_board_selection();
        self.compose_presentation = ComposePresentation::Prompt;
        self.insertion_focus = InsertionFocus::Inactive;
        self.insertion_confirmation = InsertionConfirmation::Idle;
        let thought_id = ids.thought_id();
        let (content, annotations, verified_paths, preserve_owned) = payload.into_parts();
        let action = create_action(
            thought_id,
            ids.operation_id(),
            content,
            annotations,
            CreationHistory::Board { insertion_index },
            clock.now(),
            preserve_owned,
        );
        let effects = self.reduce(action);
        if matches!(
            self.state.mode,
            InteractionMode::Edit {
                thought_id: active
            } if active == thought_id
        ) {
            self.board_viewport = self.board_viewport.follow_focus();
            self.scroll_geometry = None;
            self.layout = None;
        }
        self.state
            .attachments
            .mark_paths_accessible(thought_id, &verified_paths);
        self.sync_editor_from_state();
        effects
    }
}

pub(super) fn create_action(
    thought_id: ThoughtId,
    operation_id: OperationId,
    content: String,
    annotations: Vec<ContentAnnotation>,
    history: CreationHistory,
    at: Timestamp,
    preserve_owned: bool,
) -> Action {
    match history {
        CreationHistory::Compose {
            cursor,
            selection_anchor,
        } => Action::CreateComposeThought {
            thought_id,
            operation_id,
            content,
            annotations,
            cursor,
            selection_anchor,
            preserve_owned,
            at,
        },
        CreationHistory::Board { insertion_index } if preserve_owned => {
            Action::CreateOwnedThought(crate::application::OwnedThoughtCreation::preserved(
                thought_id,
                operation_id,
                content,
                annotations,
                insertion_index,
                at,
            ))
        }
        CreationHistory::Board { insertion_index } => Action::CreateThought {
            thought_id,
            operation_id,
            content,
            annotations,
            insertion_index,
            at,
        },
    }
}
