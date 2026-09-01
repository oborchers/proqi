//! Shared deterministic constructors for complete-board UI behavior modules.

use proqi::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, FirstRunEnvironment, first_run_board},
    domain::{ContentAnnotation, Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{environment::IdGenerator as _, store::SessionSnapshot},
    ui::{BoardApp, UiSettings},
};

use super::Fixture;

impl Fixture {
    pub(super) fn first_run(environment: FirstRunEnvironment) -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-ui-first-run"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let board = first_run_board(session, &mut ids, environment).expect("practice board");
        let state = AppState::from_snapshot(SessionSnapshot {
            board: board.board().clone(),
            board_operations: Vec::new(),
            board_history_cursor: 0,
            revisions: Vec::new(),
            editor_history_cursors: Vec::new(),
            integration_context: None,
        })
        .expect("rehydrate practice board");
        Self {
            app: BoardApp::with_settings(state, UiSettings::default(), RopeEditorFactory),
            ids,
            clock: FakeClock::new(Timestamp::from_millis(20)),
        }
    }

    /// Seed durable metadata directly for projection tests. This models same-user
    /// store bytes and intentionally bypasses every supported authoring surface.
    pub(super) fn with_annotated_thought(
        content: &str,
        annotations: Vec<ContentAnnotation>,
    ) -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-ui-annotated-contract"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let mut thought = Thought::new(
            ids.thought_id(),
            session.id,
            content.to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(10),
        );
        thought
            .set_annotations(annotations)
            .expect("valid direct durable fixture");
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        Self {
            app: BoardApp::new(AppState::new(board), RopeEditorFactory),
            ids,
            clock: FakeClock::new(Timestamp::from_millis(20)),
        }
    }
}
