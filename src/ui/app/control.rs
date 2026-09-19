//! Owner-control mutations applied through the same reducer as terminal input.

mod transformations;

use super::{BoardApp, SessionRenamePersistence};
use crate::{
    application::{
        Action, ApplicationError, Effect, InteractionMode, OwnedThoughtCreation,
        exact_live_thought, reduce,
    },
    domain::{BoardOperationKind, OperationId, TextPosition, ThoughtId, Timestamp, UndoScope},
    ports::{control::ControlMutation, environment::Clock},
};

impl BoardApp {
    /// Resolve owner-local settings that participate in one semantic request.
    pub(crate) fn canonical_control_mutation(&self, mutation: &ControlMutation) -> ControlMutation {
        let mut mutation = mutation.clone();
        if let ControlMutation::MergeThoughts { separator, .. } = &mut mutation {
            separator.clone_from(&self.settings.merge_separator);
        }
        mutation
    }

    /// Apply one typed active-owner mutation and return its ordered persistence effect.
    pub(crate) fn handle_control(
        &mut self,
        mutation: &ControlMutation,
        clock: &impl Clock,
    ) -> Result<Vec<Effect>, ApplicationError> {
        if self.session_rename_persistence.is_saving()
            && matches!(mutation, ControlMutation::RenameSession { .. })
        {
            return Err(ApplicationError::InvalidState);
        }
        let previous_mode = self.state.mode;
        let previous_focus = self.state.focused_item;
        let at = clock.now();
        let Some(action) = self.control_action(mutation, at)? else {
            return Ok(Vec::new());
        };
        let effects = reduce(&mut self.state, action)?;
        if matches!(mutation, ControlMutation::RenameSession { .. })
            && effects
                .iter()
                .any(|effect| matches!(effect, Effect::CommitBrowserOperation(_)))
        {
            self.session_rename_persistence = SessionRenamePersistence::Saving;
        }
        self.restore_live_interaction(previous_mode, previous_focus);
        self.reconcile_thought_rename();
        self.sync_editor_from_state();
        Ok(effects)
    }

    /// Restore the reducer state when owner-control effect validation rejects a mutation.
    pub(crate) fn restore_control_state(&mut self, state: crate::application::AppState) {
        self.state = state;
        self.sync_editor_from_state();
    }

    fn control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Option<Action>, ApplicationError> {
        let action = match mutation {
            ControlMutation::RenameSession { operation_id, name } => Action::RenameSession {
                operation_id: *operation_id,
                name: name.clone(),
                at,
            },
            ControlMutation::Sync => return Ok(None),
            mutation @ (ControlMutation::Replace { .. }
            | ControlMutation::Add { .. }
            | ControlMutation::PreserveAdd { .. }) => self.content_control_action(mutation, at)?,
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
            ControlMutation::RenameThought {
                operation_id,
                thought_id,
                name,
            } => Action::RenameThought {
                operation_id: *operation_id,
                thought_id: *thought_id,
                name: name.clone(),
                at,
            },
            mutation @ (ControlMutation::InsertSeparator { .. }
            | ControlMutation::DeleteItems { .. }
            | ControlMutation::MoveItem { .. }
            | ControlMutation::DuplicateItems { .. }) => self.item_control_action(mutation, at)?,
            mutation @ (ControlMutation::SplitThought { .. }
            | ControlMutation::ExtractThought { .. }
            | ControlMutation::MergeThoughts { .. }
            | ControlMutation::ReflowThought { .. }) => {
                return self.transformation_control_action(mutation, at);
            }
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
            | ControlMutation::UpdateQuiesce { .. }
            | ControlMutation::UpdateRestart { .. }
            | ControlMutation::CaptureTakeover { .. } => {
                return Err(ApplicationError::InvalidState);
            }
        };
        Ok(Some(action))
    }

    fn item_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Action, ApplicationError> {
        match mutation {
            ControlMutation::InsertSeparator {
                operation_id,
                separator_id,
                position,
            } => {
                let expected =
                    crate::domain::SeparatorId::from_database_bytes(operation_id.database_bytes())
                        .map_err(|_| ApplicationError::InvalidState)?;
                if expected != *separator_id {
                    return Err(ApplicationError::InvalidState);
                }
                Ok(Action::InsertSeparator {
                    operation_id: *operation_id,
                    separator_id: *separator_id,
                    insertion_index: position
                        .unwrap_or_else(|| self.state.board.live_items().len()),
                    at,
                })
            }
            ControlMutation::DeleteItems {
                operation_id,
                item_ids,
            } => Ok(Action::DeleteItems {
                operation_id: *operation_id,
                item_ids: item_ids.clone(),
                kind: BoardOperationKind::Delete,
                at,
            }),
            ControlMutation::MoveItem {
                operation_id,
                item_id,
                position,
            } => Ok(Action::MoveItem {
                operation_id: *operation_id,
                item_id: *item_id,
                to: *position,
                at,
            }),
            ControlMutation::DuplicateItems {
                operation_id,
                item_ids,
                duplicate_ids,
            } => {
                let expected =
                    crate::application::derived_duplicate_item_ids(*operation_id, item_ids)
                        .map_err(|_| ApplicationError::InvalidState)?;
                if expected != *duplicate_ids {
                    return Err(ApplicationError::InvalidState);
                }
                Ok(Action::DuplicateItems {
                    operation_id: *operation_id,
                    item_ids: item_ids.clone(),
                    duplicate_ids: duplicate_ids.clone(),
                    at,
                })
            }
            _ => Err(ApplicationError::InvalidState),
        }
    }

    fn content_control_action(
        &self,
        mutation: &ControlMutation,
        at: Timestamp,
    ) -> Result<Action, ApplicationError> {
        match mutation {
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
            ),
            ControlMutation::Add {
                operation_id,
                thought_id,
                content,
                annotations,
                position,
            } => {
                if annotations
                    .iter()
                    .any(crate::domain::ContentAnnotation::is_shortcut_emphasis)
                {
                    return Err(ApplicationError::InvalidState);
                }
                Ok(create_action(
                    *operation_id,
                    *thought_id,
                    content,
                    annotations,
                    *position,
                    at,
                ))
            }
            ControlMutation::PreserveAdd {
                operation_id,
                thought_id,
                content,
                annotations,
                name,
                position,
            } => Ok(Action::CreateOwnedThought(OwnedThoughtCreation::preserved(
                *thought_id,
                *operation_id,
                content.clone(),
                annotations.clone(),
                name.clone(),
                *position,
                at,
            ))),
            _ => Err(ApplicationError::InvalidState),
        }
    }

    fn replacement_action(
        &self,
        revision_id: crate::domain::RevisionId,
        thought_id: ThoughtId,
        expected_digest: Option<[u8; 32]>,
        content: String,
        at: Timestamp,
    ) -> Result<Action, ApplicationError> {
        let thought = exact_live_thought(&self.state, thought_id, expected_digest)?;
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
        previous_focus: Option<crate::domain::BoardItemId>,
    ) {
        let live_focus = previous_focus.filter(|id| self.state.board.item_position(*id).is_some());
        self.state.mode = match previous_mode {
            InteractionMode::Compose => InteractionMode::Compose,
            InteractionMode::Edit { thought_id }
                if live_focus == Some(crate::domain::BoardItemId::Thought(thought_id)) =>
            {
                InteractionMode::Edit { thought_id }
            }
            InteractionMode::Board | InteractionMode::Edit { .. } => InteractionMode::Board,
        };
        self.state.focused_item = live_focus;
    }
}

fn create_action(
    operation_id: OperationId,
    thought_id: ThoughtId,
    content: &str,
    annotations: &[crate::domain::ContentAnnotation],
    position: Option<usize>,
    at: Timestamp,
) -> Action {
    Action::CreateThought {
        thought_id,
        operation_id,
        content: content.to_owned(),
        annotations: annotations.to_vec(),
        insertion_index: position,
        at,
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
