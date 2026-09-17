//! Symmetric blocked vertical navigation and explicit boundary insertion.

use crate::{
    application::{Action, Effect},
    domain::ThoughtId,
    ports::{
        editor::{CursorMovement, EditorSnapshot},
        environment::{Clock, IdGenerator},
    },
    ui::{PastePayload, PointerKind},
};

use super::{BoardApp, BoundaryInsertion, InsertionConfirmation, InsertionFocus, UiInput};

impl BoardApp {
    pub(super) fn reset_insertion_confirmation(&mut self, input: &UiInput) {
        if matches!(
            input,
            UiInput::Pointer(crate::ui::PointerInput {
                kind: PointerKind::Move,
                ..
            })
        ) {
            return;
        }
        let boundary = match input {
            UiInput::Key(key) => match self.settings.shortcuts.board_action_for_intention(*key) {
                Some(crate::ui::ShortcutActionId::FocusPrevious) => {
                    Some(BoundaryInsertion::BeforeFirst)
                }
                Some(crate::ui::ShortcutActionId::FocusNext) => Some(BoundaryInsertion::AfterLast),
                _ => None,
            },
            _ => None,
        };
        let continues = boundary.is_some_and(|boundary| self.at_boundary(boundary));
        if !continues {
            self.insertion_confirmation = InsertionConfirmation::Idle;
        }
    }

    pub(super) fn confirm_boundary_creation(
        &mut self,
        boundary: BoundaryInsertion,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.insertion_confirmation == InsertionConfirmation::Armed(boundary) {
            match boundary {
                BoundaryInsertion::BeforeFirst => self.create_blank_at(0, ids, clock),
                BoundaryInsertion::AfterLast => self.create_blank_at_bottom(ids, clock),
            }
        } else {
            self.insertion_confirmation = InsertionConfirmation::Armed(boundary);
            Vec::new()
        }
    }

    pub(super) fn create_blank_at_bottom(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let insertion_index = self.bottom_insertion_index();
        if insertion_index == 0 {
            self.enter_provisional_compose()
        } else {
            self.create_blank_at(insertion_index, ids, clock)
        }
    }

    pub(super) fn create_at_bottom(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let insertion_index = self.bottom_insertion_index();
        self.create_at(payload, insertion_index, ids, clock)
    }

    fn bottom_insertion_index(&self) -> usize {
        self.state.board.live_items().len()
    }

    pub(super) fn insert_relative_to_focus(
        &mut self,
        below: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.insertion_focused() {
            self.set_warning("focus a Board item before inserting above or below");
            return Vec::new();
        }
        let Some(reference) = self.state.focused_item else {
            self.set_warning("focus a Board item before inserting above or below");
            return Vec::new();
        };
        if reference
            .thought()
            .is_some_and(|thought_id| self.submission_locked(thought_id))
        {
            self.set_warning("focused thought has a submission in progress");
            return Vec::new();
        }
        let live = self.state.board.live_items();
        let Some(index) = live.iter().position(|item| item.id() == reference) else {
            self.set_warning("focused Board item is no longer available");
            return Vec::new();
        };
        self.create_blank_at(index.saturating_add(usize::from(below)), ids, clock)
    }

    pub(super) fn at_first_thought(&self) -> bool {
        !self.insertion_focused()
            && self
                .state
                .board
                .live_items()
                .first()
                .map(|item| item.id())
                .is_some_and(|id| self.state.focused_item == Some(id))
    }

    fn at_boundary(&self, boundary: BoundaryInsertion) -> bool {
        match boundary {
            BoundaryInsertion::BeforeFirst => self.at_first_thought(),
            BoundaryInsertion::AfterLast => self.insertion_focused(),
        }
    }

    pub(super) fn finish_boundary_navigation(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
        before: Option<&EditorSnapshot>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if extend_selection
            || !matches!(
                movement,
                CursorMovement::VisualUp | CursorMovement::VisualDown
            )
        {
            self.edit_boundary = None;
            return Vec::new();
        }
        let (Some(before), Some(after)) = (before, self.editor_snapshot()) else {
            return Vec::new();
        };
        if before.cursor != after.cursor || before.selection != after.selection {
            self.edit_boundary = None;
            return Vec::new();
        }
        let armed = self.edit_boundary == Some(movement);
        self.edit_boundary = Some(movement);
        if !armed {
            return Vec::new();
        }
        self.complete_boundary_navigation(movement, after.content.is_empty(), ids, clock)
    }

    fn complete_boundary_navigation(
        &mut self,
        movement: CursorMovement,
        empty: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let target = self.edit_neighbor(movement);
        if target.is_none() && empty {
            self.edit_boundary = None;
            return Vec::new();
        }
        let mut effects = self.finish_edit(ids, clock);
        if self.pending_edit.is_some() {
            return effects;
        }
        self.palette_selection_handoff = None;
        if let Some(target) = target {
            self.insertion_focus = InsertionFocus::Inactive;
            effects.extend(self.reduce(Action::FocusThought(Some(target))));
        } else {
            let boundary_effects = match movement {
                CursorMovement::VisualUp => self.create_blank_at(0, ids, clock),
                CursorMovement::VisualDown => self.create_blank_at_bottom(ids, clock),
                _ => return effects,
            };
            effects.extend(boundary_effects);
        }
        effects
    }

    fn edit_neighbor(&self, movement: CursorMovement) -> Option<ThoughtId> {
        let live = self.state.board.live_thoughts();
        let active = self.active_thought_id()?;
        let current = live.iter().position(|thought| thought.id == active)?;
        let target = match movement {
            CursorMovement::VisualUp => current.checked_sub(1)?,
            CursorMovement::VisualDown => current.saturating_add(1),
            _ => return None,
        };
        live.get(target).map(|thought| thought.id)
    }
}
