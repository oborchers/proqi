//! Symmetric blocked vertical navigation and explicit boundary insertion.

use crate::{
    application::{Action, Effect},
    domain::ThoughtId,
    ports::{
        editor::{CursorMovement, EditorSnapshot},
        environment::{Clock, IdGenerator},
    },
    ui::{PastePayload, UiInput, UiKey, settings::BoardCommand},
};

use super::{BoardApp, BoundaryInsertion, InsertionConfirmation, InsertionFocus};

impl BoardApp {
    pub(super) fn reset_insertion_confirmation(&mut self, input: &UiInput) {
        let boundary = match input {
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection: false,
            }) => BoundaryInsertion::for_movement(*movement),
            UiInput::Key(UiKey::Character(character)) => {
                BoundaryInsertion::for_command(self.settings.keybindings.command(*character))
            }
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
                BoundaryInsertion::BeforeFirst => {
                    self.create_at(PastePayload::text(String::new()), 0, ids, clock)
                }
                BoundaryInsertion::AfterLast => self.begin_insertion(ids, clock),
            }
        } else {
            self.insertion_confirmation = InsertionConfirmation::Armed(boundary);
            Vec::new()
        }
    }

    pub(super) fn at_first_thought(&self) -> bool {
        !self.insertion_focused()
            && self
                .state
                .board
                .live_thoughts()
                .first()
                .map(|thought| thought.id)
                == self.state.focused_thought
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
            let insertion_index = match movement {
                CursorMovement::VisualUp => 0,
                CursorMovement::VisualDown => self.state.board.live_thoughts().len(),
                _ => return effects,
            };
            effects.extend(self.create_at(
                PastePayload::text(String::new()),
                insertion_index,
                ids,
                clock,
            ));
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

impl BoundaryInsertion {
    const fn for_movement(movement: CursorMovement) -> Option<Self> {
        match movement {
            CursorMovement::VisualUp => Some(Self::BeforeFirst),
            CursorMovement::VisualDown => Some(Self::AfterLast),
            _ => None,
        }
    }

    const fn for_command(command: Option<BoardCommand>) -> Option<Self> {
        match command {
            Some(BoardCommand::FocusUp) => Some(Self::BeforeFirst),
            Some(BoardCommand::FocusDown) => Some(Self::AfterLast),
            _ => None,
        }
    }
}
