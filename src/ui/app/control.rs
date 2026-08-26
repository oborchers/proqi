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
        if let Some(thought_id) = mutation.thought_id()
            && self.submission_locked(thought_id)
        {
            return Err(ApplicationError::ThoughtLocked(thought_id));
        }
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
            } => Action::SetCollapsed {
                operation_id: *operation_id,
                thought_id: *thought_id,
                collapsed: *collapsed,
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

    #[test]
    fn external_replace_is_an_editor_revision_that_undoes_normally() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-control-replace"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let thought_id = ids.thought_id();
        let thought = Thought::new(
            thought_id,
            session.id,
            "before".to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        let mut app = BoardApp::new(
            AppState::new(board),
            crate::adapters::editor::RopeEditorFactory,
        );
        let clock = FakeClock::new(Timestamp::from_millis(2));

        let effects = app
            .handle_control(
                &ControlMutation::Replace {
                    revision_id: ids.revision_id(),
                    thought_id,
                    expected_digest: None,
                    content: "after".to_owned(),
                },
                &clock,
            )
            .expect("replace");
        assert!(matches!(
            effects.as_slice(),
            [crate::application::Effect::CommitRevision(_)]
        ));
        assert_eq!(
            app.state
                .board
                .thought(thought_id)
                .expect("thought")
                .content,
            "after"
        );

        app.handle_control(
            &ControlMutation::History {
                operation_id: ids.operation_id(),
                scope: crate::domain::UndoScope::Editor { thought_id },
                undo: true,
            },
            &clock,
        )
        .expect("undo replacement");
        assert_eq!(
            app.state
                .board
                .thought(thought_id)
                .expect("thought")
                .content,
            "before"
        );
    }

    #[test]
    fn thought_mutations_are_locked_from_submission_intent_until_completion() {
        use crate::{
            application::ApplicationError,
            domain::Direction,
            ports::agent::{
                AgentDeliveryCapabilities, AgentState, AgentTarget, PaneContext, PaneRect,
            },
            ui::UiKey,
        };

        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-control-lock"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let thought_id = ids.thought_id();
        let thought = Thought::new(
            thought_id,
            session.id,
            "locked".to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let mut app = BoardApp::new(
            AppState::new(SessionBoard::new(session, vec![thought]).expect("board")),
            crate::adapters::editor::RopeEditorFactory,
        );
        let source = PaneContext {
            workspace_id: "workspace".to_owned(),
            tab_id: "tab".to_owned(),
            pane_id: "source".to_owned(),
            rect: PaneRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        };
        app.complete_agent_discovery(Ok(vec![AgentTarget {
            provider: "herdr".to_owned(),
            protocol: 1,
            direction: Direction::Left,
            pane_id: "target".to_owned(),
            workspace_id: source.workspace_id.clone(),
            tab_id: source.tab_id.clone(),
            agent_kind: "codex".to_owned(),
            agent_name: "Codex".to_owned(),
            agent_session_id: "agent-session".to_owned(),
            readiness: AgentState::Working,
            delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
            rect: PaneRect {
                x: 10,
                y: 0,
                width: 10,
                height: 10,
            },
            source,
        }]));
        let clock = FakeClock::new(Timestamp::from_millis(2));
        let effects = app.handle(UiInput::Key(UiKey::Character('s')), &mut ids, &clock);
        assert!(matches!(
            effects.as_slice(),
            [crate::application::Effect::PrepareSubmission(_)]
        ));

        let error = app
            .handle_control(
                &ControlMutation::Delete {
                    operation_id: ids.operation_id(),
                    thought_id,
                },
                &clock,
            )
            .expect_err("locked thought");
        assert_eq!(error, ApplicationError::ThoughtLocked(thought_id));
    }
}
