//! Owner-control mutations applied through the same reducer as terminal input.

use crate::{
    application::{Action, ApplicationError, Effect, reduce},
    domain::BoardOperationKind,
    ports::{control::ControlMutation, environment::Clock},
};

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
        let action = match mutation {
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
            } => {
                if *undo {
                    Action::Undo {
                        operation_id: *operation_id,
                        scope: *scope,
                        at,
                    }
                } else {
                    Action::Redo {
                        operation_id: *operation_id,
                        scope: *scope,
                        at,
                    }
                }
            }
            ControlMutation::UpdatePrepare { .. }
            | ControlMutation::UpdateRelease { .. }
            | ControlMutation::UpdateRestart { .. } => {
                return Err(ApplicationError::InvalidState);
            }
        };
        let effects = reduce(&mut self.state, action)?;
        self.restore_live_interaction(previous_mode, previous_focus);
        self.sync_editor_from_state();
        Ok(effects)
    }

    fn restore_live_interaction(
        &mut self,
        previous_mode: crate::application::InteractionMode,
        previous_focus: Option<crate::domain::ThoughtId>,
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
            crate::application::InteractionMode::Edit { thought_id } if thought_id == focus => {
                previous_mode
            }
            _ => crate::application::InteractionMode::Board,
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::{FakeClock, FakeIdGenerator},
        application::{AppState, InteractionMode},
        domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
        ports::{control::ControlMutation, editor::EditCommand, environment::IdGenerator},
        ui::UiInput,
    };

    use super::BoardApp;

    #[test]
    fn active_add_preserves_the_users_live_editor_and_focus() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-control-focus"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let original_id = ids.thought_id();
        let original = Thought::new(
            original_id,
            session.id,
            "editing".to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let board = SessionBoard::new(session, vec![original]).expect("board");
        let mut app = BoardApp::new(
            AppState::new(board),
            crate::adapters::editor::RopeEditorFactory,
        );
        app.state.mode = InteractionMode::Edit {
            thought_id: original_id,
        };
        app.sync_editor_from_state();
        app.apply_edit(EditCommand::InsertChar('!'));
        let editor_before = app.editor_snapshot().expect("live editor draft");
        assert!(app.has_pending_edit());
        let added_id = ids.thought_id();
        let mutation = ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: added_id,
            content: "external".to_owned(),
            annotations: Vec::new(),
            position: None,
        };

        let effects = app
            .handle_control(&mutation, &FakeClock::new(Timestamp::from_millis(2)))
            .expect("control add");

        assert_eq!(effects.len(), 1);
        assert_eq!(app.editor_snapshot(), Some(editor_before));
        assert!(app.has_pending_edit());
        assert_eq!(app.state.focused_thought, Some(original_id));
        assert_eq!(
            app.state.mode,
            InteractionMode::Edit {
                thought_id: original_id
            }
        );
        assert_eq!(
            app.state
                .board
                .thought(added_id)
                .expect("added thought")
                .content,
            "external"
        );
    }

    #[test]
    fn ui_paste_and_forwarded_add_produce_the_same_state_and_durable_effect() {
        let mut session_ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            session_ids.session_id(),
            std::env::temp_dir().join("proqi-entry-point-conformance"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        let state = AppState::new(board);
        let mut ui = BoardApp::new(state.clone(), crate::adapters::editor::RopeEditorFactory);
        let mut forwarded = BoardApp::new(state, crate::adapters::editor::RopeEditorFactory);
        let clock = FakeClock::new(Timestamp::from_millis(2));
        let mut ui_ids = FakeIdGenerator::new(1_725_300_000_000);
        let mut forwarded_ids = FakeIdGenerator::new(1_725_300_000_000);
        let thought_id = forwarded_ids.thought_id();
        let operation_id = forwarded_ids.operation_id();

        let ui_effects = ui.handle(
            UiInput::Paste("same content".to_owned()),
            &mut ui_ids,
            &clock,
        );
        let forwarded_effects = forwarded
            .handle_control(
                &ControlMutation::Add {
                    operation_id,
                    thought_id,
                    content: "same content".to_owned(),
                    annotations: Vec::new(),
                    position: None,
                },
                &clock,
            )
            .expect("forwarded add");

        assert_eq!(forwarded.state, ui.state);
        assert_eq!(forwarded_effects, ui_effects);
        assert_eq!(forwarded_effects.len(), 1);
        assert!(forwarded_effects[0].persistence_batch().is_some());
    }

    #[test]
    fn external_editor_undo_refreshes_content_and_restores_cursor() {
        use crate::{domain::TextPosition, ports::editor::EditCommand};

        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-control-editor"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let thought_id = ids.thought_id();
        let thought = Thought::new(
            thought_id,
            session.id,
            "base".to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        let mut app = BoardApp::new(
            AppState::new(board),
            crate::adapters::editor::RopeEditorFactory,
        );
        app.state.mode = InteractionMode::Edit { thought_id };
        app.sync_editor_from_state();
        app.apply_edit(EditCommand::Paste(" changed".to_owned()));
        let _effects = app.flush_pending_edit(&mut ids, &FakeClock::new(Timestamp::from_millis(2)));
        let mutation = ControlMutation::History {
            operation_id: ids.operation_id(),
            scope: crate::domain::UndoScope::Editor { thought_id },
            undo: true,
        };

        app.handle_control(&mutation, &FakeClock::new(Timestamp::from_millis(3)))
            .expect("external undo");

        let snapshot = app.editor_snapshot().expect("editor");
        assert_eq!(snapshot.content, "base");
        assert_eq!(snapshot.cursor, TextPosition::new(0, 4));
    }
}
