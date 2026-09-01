//! Edit-owned command-palette execution.

use crate::{
    application::{Effect, InteractionMode},
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::{Command, EditorSelectionHandoff};
use crate::ui::app::{BoardApp, UiKey};

impl BoardApp {
    pub(super) fn execute_editor_command(
        &mut self,
        command: Command,
        selection_handoff: Option<EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if !editor_owned(command) {
            return None;
        }
        let mut effects = if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            Vec::new()
        } else {
            self.expand_and_enter_edit(ids, clock)
        };
        self.restore_palette_selection_handoff(selection_handoff);
        if command == Command::PlainNewline {
            effects.extend(self.insert_newline(false, ids, clock));
            return Some(effects);
        }
        if matches!(
            command,
            Command::DeleteLogicalLine | Command::DeleteSentence
        ) {
            let key = if command == Command::DeleteLogicalLine {
                UiKey::DeleteLogicalLine
            } else {
                UiKey::DeleteSentence
            };
            effects.extend(self.handle_edit_key(key, ids, clock));
            return Some(effects);
        }
        if let Some(movement) = movement(command) {
            effects.extend(self.handle_edit_key(
                UiKey::Move {
                    movement,
                    extend_selection: false,
                },
                ids,
                clock,
            ));
            return Some(effects);
        }
        effects.extend(self.apply_indentation(command == Command::Outdent, ids, clock));
        Some(effects)
    }
}

fn editor_owned(command: Command) -> bool {
    matches!(
        command,
        Command::PlainNewline
            | Command::DeleteLogicalLine
            | Command::DeleteSentence
            | Command::JumpUp
            | Command::JumpDown
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent
    )
}

const fn movement(command: Command) -> Option<CursorMovement> {
    match command {
        Command::JumpUp => Some(CursorMovement::VisualJumpUp),
        Command::JumpDown => Some(CursorMovement::VisualJumpDown),
        Command::ThoughtStart => Some(CursorMovement::DocumentStart),
        Command::ThoughtEnd => Some(CursorMovement::DocumentEnd),
        _ => None,
    }
}
