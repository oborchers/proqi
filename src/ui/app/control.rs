//! Owner-control mutations applied through the same reducer as terminal input.

use crate::{
    application::{Action, ApplicationError, Effect, InteractionMode, reduce},
    domain::{BoardOperationKind, OperationId, TextPosition, ThoughtId, Timestamp, UndoScope},
    ports::{control::ControlMutation, environment::Clock},
};
use sha2::{Digest as _, Sha256};

use super::BoardApp;

impl BoardApp {
    /// Apply one typed active-owner mutation and return its ordered persistence effect.
    pub(crate) fn handle_control(
        &mut self,
        mutation: &ControlMutation,
        clock: &impl Clock,
    ) -> Result<Vec<Effect>, ApplicationError> {
        let previous_mode = self.state.mode;
        let previous_focus = self.state.focused_thought;
        let at = clock.now();
        let Some(action) = self.control_action(mutation, at)? else {
            return Ok(Vec::new());
        };
        let effects = reduce(&mut self.state, action)?;
        self.restore_live_interaction(previous_mode, previous_focus);
        self.sync_editor_from_state();
        Ok(effects)
    }

    fn control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let action = match mutation {
            ControlMutation::RenameSession { name } => Action::RenameSession { name: name.clone() },
            ControlMutation::Sync => return Ok(None),
            ControlMutation::Replace {
                revision_id,
                thought_id,
                expected_digest,
                content,
            } => self.replacement_action(
                *revision_id,
                *thought_id,
                *expected_digest,
                content.clone(),
                at,
            )?,
            ControlMutation::SetCollapsed {
                operation_id,
                thought_id,
                collapsed,
            } => Action::SetPresentation {
                operation_id: *operation_id,
                thought_id: *thought_id,
                presentation: if *collapsed {
                    crate::domain::ThoughtPresentation::Collapsed
                } else {
                    crate::domain::ThoughtPresentation::Automatic
                },
                at,
            },
            ControlMutation::Add {
                operation_id,
                thought_id,
                content,
                annotations,
                position,
            } => Action::CreateThought {
                thought_id: *thought_id,
                operation_id: *operation_id,
                content: content.clone(),
                annotations: annotations.clone(),
                insertion_index: *position,
                at,
            },
            ControlMutation::Delete {
                operation_id,
                thought_id,
            } => Action::DeleteThought {
                operation_id: *operation_id,
                thought_id: *thought_id,
                kind: BoardOperationKind::Delete,
                at,
            },
            ControlMutation::Move {
                operation_id,
                thought_id,
                position,
            } => Action::MoveThought {
                operation_id: *operation_id,
                thought_id: *thought_id,
                to: *position,
                at,
            },
            ControlMutation::History {
                operation_id,
                scope,
                undo,
            } => history_action(*operation_id, *scope, *undo, at),
            ControlMutation::UpdatePrepare { .. }
            | ControlMutation::UpdateRelease { .. }
            | ControlMutation::UpdateRestart { .. } => {
                return Err(ApplicationError::InvalidState);
            }
        };
        Ok(Some(action))
    }

    fn replacement_action(
        &self,
        revision_id: crate::domain::RevisionId,
        thought_id: ThoughtId,
        expected_digest: Option<[u8; 32]>,
        content: String,
        at: Timestamp,
    ) -> Result<Action, ApplicationError> {
        let thought = self
            .state
            .board
            .thought(thought_id)
            .filter(|thought| thought.is_live())
            .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
        let current_digest: [u8; 32] = Sha256::digest(thought.content.as_bytes()).into();
        if expected_digest.is_some_and(|expected| expected != current_digest) {
            return Err(ApplicationError::ContentConflict(thought_id));
        }
        Ok(Action::EditThought {
            thought_id,
            revision_id,
            before_content: thought.content.clone(),
            after_content: content,
            before_annotations: thought.annotations.clone(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::default(),
            after_cursor: TextPosition::default(),
            at,
        })
    }

    fn restore_live_interaction(
        &mut self,
        previous_mode: InteractionMode,
        previous_focus: Option<ThoughtId>,
    ) {
        let Some(focus) = previous_focus.filter(|id| {
            self.state
                .board
                .thought(*id)
                .is_some_and(crate::domain::Thought::is_live)
        }) else {
            return;
        };
        self.state.focused_thought = Some(focus);
        self.state.mode = match previous_mode {
            InteractionMode::Edit { thought_id } if thought_id == focus => previous_mode,
            _ => InteractionMode::Board,
        };
    }
}

fn history_action(
    operation_id: OperationId,
    scope: UndoScope,
    undo: bool,
    at: Timestamp,
) -> Action {
    if undo {
        Action::Undo {
            operation_id,
            scope,
            at,
        }
    } else {
        Action::Redo {
            operation_id,
            scope,
            at,
        }
    }
}

#[cfg(test)]
mod tests;
