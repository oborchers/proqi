//! Model and reducer contracts for board operations, editor history, and effects.

use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{
        Action, AppState, ClipboardIntent, DurabilityState, Effect, FailureCode, InteractionMode,
        reduce,
    },
    domain::{
        BoardMutation, BoardOperationKind, ContentAnnotation, ContentAnnotationKind,
        OperationSequence, Session, SessionBoard, TextPosition, ThoughtId, ThoughtPosition,
        ThoughtPresentation, Timestamp, UndoScope,
    },
    ports::environment::IdGenerator,
};

struct Fixture {
    ids: FakeIdGenerator,
    state: AppState,
    now: i64,
}

impl Fixture {
    fn new() -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-contract"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        Self {
            ids,
            state: AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            now: 1,
        }
    }

    fn time(&mut self) -> Timestamp {
        self.now += 1;
        Timestamp::from_millis(self.now)
    }

    fn create(&mut self, content: &str) -> ThoughtId {
        let thought_id = self.ids.thought_id();
        let operation_id = self.ids.operation_id();
        let at = self.time();
        reduce(
            &mut self.state,
            Action::CreateThought {
                thought_id,
                operation_id,
                content: content.to_owned(),
                annotations: Vec::new(),
                insertion_index: None,
                at,
            },
        )
        .expect("create");
        thought_id
    }

    fn operation_id(&mut self) -> proqi::domain::OperationId {
        self.ids.operation_id()
    }
}

fn move_history(fixture: &mut Fixture, scope: UndoScope, undo: bool) {
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let action = if undo {
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
    };
    reduce(&mut fixture.state, action).expect("history move");
}

#[path = "domain_reducer/clipboard.rs"]
mod clipboard;
#[path = "domain_reducer/history.rs"]
mod history;
#[path = "domain_reducer/locks.rs"]
mod locks;
#[path = "domain_reducer/top_boundary.rs"]
mod top_boundary;
#[path = "domain_reducer/transformations.rs"]
mod transformations;

#[test]
fn interaction_mode_is_explicit_and_empty_board_policy_is_typed() {
    let mut fixture = Fixture::new();
    assert_eq!(fixture.state.mode, InteractionMode::Compose);
    assert!(
        reduce(&mut fixture.state, Action::ExitCompose)
            .expect("board")
            .is_empty()
    );
    assert_eq!(fixture.state.mode, InteractionMode::Board);

    fixture
        .state
        .reconcile_empty_board(proqi::application::EmptyBoardTransition::Preserve);
    assert_eq!(fixture.state.mode, InteractionMode::Board);
    fixture
        .state
        .reconcile_empty_board(proqi::application::EmptyBoardTransition::ComposeAfterLocalRemoval);
    assert_eq!(fixture.state.mode, InteractionMode::Compose);
    assert!(
        reduce(&mut fixture.state, Action::EnterCompose)
            .expect("compose")
            .is_empty()
    );
}
