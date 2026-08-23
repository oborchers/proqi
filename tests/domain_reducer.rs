//! Model and reducer contracts for board operations, editor history, and effects.

use std::path::PathBuf;

use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{
        Action, AppState, ClipboardIntent, DurabilityState, Effect, FailureCode, InteractionMode,
        reduce,
    },
    domain::{
        BoardMutation, BoardOperationKind, OperationSequence, Session, SessionBoard, ThoughtId,
        ThoughtPosition, Timestamp, UndoScope,
    },
    ports::{editor::TextPosition, environment::IdGenerator},
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
            PathBuf::from("/tmp/proqi-contract"),
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

#[test]
fn paste_creates_one_focused_thought_and_one_commit_effect() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let effects = reduce(
        &mut fixture.state,
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content: "Grüße\r\n日本語".to_owned(),
            at,
        },
    )
    .expect("paste");

    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], Effect::CommitBoardOperation(_)));
    assert_eq!(fixture.state.focused_thought, Some(thought_id));
    assert_eq!(fixture.state.mode, InteractionMode::Edit { thought_id });
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "Grüße\r\n日本語"
    );
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Pending {
            durable: OperationSequence::ZERO,
            latest: OperationSequence::new(1),
        }
    );
}

#[test]
fn copy_is_exact_and_never_mutates_the_board() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("  exact\r\n");
    let request_id = fixture.ids.request_id();
    let effects = reduce(
        &mut fixture.state,
        Action::CopyThought {
            request_id,
            thought_id,
        },
    )
    .expect("copy");
    assert_eq!(
        effects,
        vec![Effect::WriteClipboard {
            request_id,
            thought_id,
            intent: ClipboardIntent::Copy,
            content: "  exact\r\n".to_owned(),
        }]
    );
    reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("clipboard result");
    assert!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );
}

#[test]
fn cut_deletes_only_after_clipboard_success() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("keep until copied");

    let failed_request = fixture.ids.request_id();
    let failed_operation = fixture.operation_id();
    let failed_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::CutThought {
            request_id: failed_request,
            operation_id: failed_operation,
            thought_id,
            at: failed_at,
        },
    )
    .expect("request cut");
    let failure = reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id: failed_request,
            result: Err(FailureCode::ClipboardFailed),
        },
    )
    .expect("failure");
    assert_eq!(
        failure,
        vec![Effect::Notify {
            code: FailureCode::ClipboardFailed
        }]
    );
    assert!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );

    let request_id = fixture.ids.request_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::CutThought {
            request_id,
            operation_id,
            thought_id,
            at,
        },
    )
    .expect("request cut");
    let success = reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("success");
    assert!(
        matches!(success.as_slice(), [Effect::CommitBoardOperation(operation)] if operation.kind == BoardOperationKind::Cut)
    );
    assert!(
        !fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );
}

#[test]
fn board_reorder_delete_and_collapse_have_independent_undo_redo() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let second = fixture.create("second");
    let move_operation = fixture.operation_id();
    let move_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::MoveThought {
            operation_id: move_operation,
            thought_id: second,
            to: 0,
            at: move_at,
        },
    )
    .expect("move");
    assert_eq!(fixture.state.board.live_thoughts()[0].id, second);

    let collapse_operation = fixture.operation_id();
    let collapse_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::SetCollapsed {
            operation_id: collapse_operation,
            thought_id: first,
            collapsed: true,
            at: collapse_at,
        },
    )
    .expect("collapse");
    assert!(fixture.state.board.thought(first).expect("first").collapsed);

    let undo_operation = fixture.operation_id();
    let undo_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id: undo_operation,
            scope: UndoScope::Board,
            at: undo_at,
        },
    )
    .expect("undo collapse");
    assert!(!fixture.state.board.thought(first).expect("first").collapsed);

    let redo_operation = fixture.operation_id();
    let redo_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Redo {
            operation_id: redo_operation,
            scope: UndoScope::Board,
            at: redo_at,
        },
    )
    .expect("redo collapse");
    assert!(fixture.state.board.thought(first).expect("first").collapsed);
    fixture.state.board.validate().expect("normalized board");

    let delete_operation = fixture.operation_id();
    let delete_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id: delete_operation,
            thought_id: second,
            kind: BoardOperationKind::Delete,
            at: delete_at,
        },
    )
    .expect("delete");
    assert!(
        !fixture
            .state
            .board
            .thought(second)
            .expect("retained")
            .is_live()
    );

    let restore_operation = fixture.operation_id();
    let restore_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id: restore_operation,
            scope: UndoScope::Board,
            at: restore_at,
        },
    )
    .expect("restore deletion");
    assert!(
        fixture
            .state
            .board
            .thought(second)
            .expect("restored")
            .is_live()
    );
    assert_eq!(fixture.state.board.live_thoughts()[0].id, second);
}

#[test]
fn editor_history_is_separate_from_board_history() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("before");
    let revision_id = fixture.ids.revision_id();
    let edit_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id,
            revision_id,
            before_content: "before".to_owned(),
            after_content: "after".to_owned(),
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 5),
            at: edit_at,
        },
    )
    .expect("edit");
    assert_eq!(fixture.state.editor_history_cursor(thought_id), 1);

    let undo_editor = fixture.operation_id();
    let undo_editor_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id: undo_editor,
            scope: UndoScope::Editor { thought_id },
            at: undo_editor_at,
        },
    )
    .expect("undo editor");
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "before"
    );
    assert_eq!(fixture.state.board_history_cursor(), 1);

    let redo_editor = fixture.operation_id();
    let redo_editor_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Redo {
            operation_id: redo_editor,
            scope: UndoScope::Editor { thought_id },
            at: redo_editor_at,
        },
    )
    .expect("redo editor");
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "after"
    );

    let undo_editor_again = fixture.operation_id();
    let undo_editor_again_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id: undo_editor_again,
            scope: UndoScope::Editor { thought_id },
            at: undo_editor_again_at,
        },
    )
    .expect("undo editor again");

    let undo_board = fixture.operation_id();
    let undo_board_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id: undo_board,
            scope: UndoScope::Board,
            at: undo_board_at,
        },
    )
    .expect("undo board create");
    assert!(
        !fixture
            .state
            .board
            .thought(thought_id)
            .expect("retained")
            .is_live()
    );
    assert_eq!(fixture.state.editor_history_cursor(thought_id), 0);
}

#[test]
fn acknowledgements_must_be_ordered_and_truthful() {
    let mut fixture = Fixture::new();
    fixture.create("one");
    fixture.create("two");
    assert!(
        reduce(
            &mut fixture.state,
            Action::PersistenceCommitted(OperationSequence::new(2))
        )
        .is_err()
    );
    assert_eq!(
        fixture.state.board.session.last_durable_sequence,
        OperationSequence::ZERO
    );

    reduce(
        &mut fixture.state,
        Action::PersistenceCommitted(OperationSequence::new(1)),
    )
    .expect("first ack");
    assert_eq!(
        fixture.state.board.session.last_durable_sequence,
        OperationSequence::new(1)
    );
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Pending {
            durable: OperationSequence::new(1),
            latest: OperationSequence::new(2),
        }
    );
    reduce(
        &mut fixture.state,
        Action::PersistenceCommitted(OperationSequence::new(2)),
    )
    .expect("second ack");
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Durable {
            sequence: OperationSequence::new(2)
        }
    );
}

#[test]
fn invalid_reducer_action_leaves_state_unchanged() {
    let mut fixture = Fixture::new();
    let missing = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let before = fixture.state.clone();
    assert!(
        reduce(
            &mut fixture.state,
            Action::DeleteThought {
                operation_id,
                thought_id: missing,
                kind: BoardOperationKind::Delete,
                at,
            },
        )
        .is_err()
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn aggregate_rejects_invalid_mutation_transactionally() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("unchanged");
    let before = fixture.state.board.clone();
    let result = fixture.state.board.apply_mutation(
        &BoardMutation::MoveThought {
            thought_id,
            from: ThoughtPosition::new(0),
            to: ThoughtPosition::new(99),
        },
        Timestamp::from_millis(99),
    );
    assert!(result.is_err());
    assert_eq!(fixture.state.board, before);
}
