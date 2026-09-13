//! Real storage failure, retry, restart, and history for the UI's reflow action.

use super::{DatabaseFixture, key_input, session_state, test_path};
use proqi::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
        sqlite::{RetryPolicy, SqliteStore},
    },
    application::{AppState, Effect},
    domain::{Timestamp, UndoScope},
    ports::store::{OperationBatch, Store, StoreError},
    ui::{BoardApp, KeyStroke, LogicalKey, LogicalModifiers, UiInput, UiKey},
};
use std::time::Duration;

fn input(
    app: &mut BoardApp,
    ids: &mut FakeIdGenerator,
    clock: &FakeClock,
    key: UiKey,
) -> Vec<Effect> {
    app.handle(key_input(key), ids, clock)
}

#[test]
fn board_reflow_and_editor_revision_keep_sequence_order_across_restarts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(100));
    let state = session_state(&mut ids, &test_path("reflow-interleaving"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let mut app = BoardApp::new(state, RopeEditorFactory);
    let effects = app.handle(UiInput::Paste("alpha".to_owned()), &mut ids, &clock);
    commit(&mut app, &mut store, &effects);
    let thought_id = app.active_thought_id().expect("thought");
    let revision = app.handle(UiInput::Paste("  beta".to_owned()), &mut ids, &clock);
    commit(&mut app, &mut store, &revision);
    let exit = input(&mut app, &mut ids, &clock, UiKey::Escape);
    assert!(exit.is_empty());
    let board_reflow = reflow(&mut app, &mut ids, &clock, true);
    commit(&mut app, &mut store, &board_reflow);
    assert_durable(&mut store, session_id, thought_id, "alpha beta");

    let (mut store, mut app) = reopen(&fixture, session_id);
    input(&mut app, &mut ids, &clock, UiKey::Enter);
    let undo_board = input(&mut app, &mut ids, &clock, UiKey::Undo);
    assert_history_scope(&undo_board, UndoScope::Board, true);
    commit(&mut app, &mut store, &undo_board);
    assert_durable(&mut store, session_id, thought_id, "alpha  beta");

    let (mut store, mut app) = reopen(&fixture, session_id);
    input(&mut app, &mut ids, &clock, UiKey::Enter);
    let undo_editor = input(&mut app, &mut ids, &clock, UiKey::Undo);
    assert_history_scope(&undo_editor, UndoScope::Editor { thought_id }, true);
    commit(&mut app, &mut store, &undo_editor);
    assert_durable(&mut store, session_id, thought_id, "alpha");

    let (mut store, mut app) = reopen(&fixture, session_id);
    input(&mut app, &mut ids, &clock, UiKey::Enter);
    let redo_editor = input(&mut app, &mut ids, &clock, UiKey::Redo);
    assert_history_scope(&redo_editor, UndoScope::Editor { thought_id }, false);
    commit(&mut app, &mut store, &redo_editor);
    assert_durable(&mut store, session_id, thought_id, "alpha  beta");

    let (mut store, mut app) = reopen(&fixture, session_id);
    input(&mut app, &mut ids, &clock, UiKey::Enter);
    let redo_board = input(&mut app, &mut ids, &clock, UiKey::Redo);
    assert_history_scope(&redo_board, UndoScope::Board, false);
    commit(&mut app, &mut store, &redo_board);
    assert_durable(&mut store, session_id, thought_id, "alpha beta");
}

fn reopen(
    fixture: &DatabaseFixture,
    session_id: proqi::domain::SessionId,
) -> (SqliteStore, BoardApp) {
    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("restart");
    let app = BoardApp::new(
        AppState::from_snapshot(snapshot).expect("restored"),
        RopeEditorFactory,
    );
    (store, app)
}

fn assert_history_scope(effects: &[Effect], expected: UndoScope, undo: bool) {
    assert!(matches!(
        effects,
        [Effect::CommitHistoryMove {
            scope,
            undo: actual_undo,
            ..
        }] if *scope == expected && *actual_undo == undo
    ));
}

fn commit(app: &mut BoardApp, store: &mut SqliteStore, effects: &[Effect]) {
    for effect in effects {
        if let Some(batch) = effect.persistence_batch()
            && let Some(receipt) = store.commit(&batch).expect("commit")
        {
            app.acknowledge_persistence(receipt.sequence, true);
        }
    }
}

fn reflow(
    app: &mut BoardApp,
    ids: &mut FakeIdGenerator,
    clock: &FakeClock,
    board: bool,
) -> Vec<Effect> {
    let modifiers = if board {
        LogicalModifiers::NONE
    } else {
        LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT)
    };
    app.handle(
        UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character('f')).with_modifiers(modifiers)),
        ids,
        clock,
    )
}

#[test]
fn reflow_failure_retry_restart_and_undo_redo_are_atomic_in_both_histories() {
    for board in [false, true] {
        let fixture = DatabaseFixture::new();
        let mut store = busy_store(&fixture);
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let clock = FakeClock::new(Timestamp::from_millis(100));
        let state = session_state(&mut ids, &test_path("reflow-restart"));
        let session_id = state.board.session.id;
        store
            .commit(&OperationBatch::CreateSession(state.board.session.clone()))
            .expect("session");
        let mut app = BoardApp::new(state, RopeEditorFactory);
        let source = "Grüße  日本語\r\nwith e\u{301} and 👩🏽‍💻";
        let effects = app.handle(UiInput::Paste(source.to_owned()), &mut ids, &clock);
        commit(&mut app, &mut store, &effects);
        let thought_id = app.active_thought_id().expect("thought");
        if board {
            let effects = input(&mut app, &mut ids, &clock, UiKey::Escape);
            commit(&mut app, &mut store, &effects);
        }
        let effects = reflow(&mut app, &mut ids, &clock, board);
        assert_eq!(effects.len(), 1);
        let batch = effects[0].persistence_batch().expect("reflow transaction");
        let sequence = batch.sequence().expect("sequence");
        let blocker = rusqlite::Connection::open(&fixture.config.database_path).expect("blocker");
        blocker.execute_batch("BEGIN IMMEDIATE").expect("lock");
        assert!(matches!(store.commit(&batch), Err(StoreError::Busy)));
        app.acknowledge_persistence(sequence, false);
        assert_durable(&mut store, session_id, thought_id, source);
        assert_eq!(
            app.state
                .board
                .thought(thought_id)
                .expect("recoverable")
                .content,
            "Grüße 日本語\r\nwith e\u{301} and 👩🏽‍💻"
        );
        let retry = input(&mut app, &mut ids, &clock, UiKey::Character('r'));
        assert_eq!(retry, [Effect::RetryPersistence { sequence }]);
        blocker.execute_batch("ROLLBACK").expect("unlock");
        let receipt = store.commit(&batch).expect("retry").expect("receipt");
        app.acknowledge_persistence(receipt.sequence, true);
        assert!(
            store
                .commit(&batch)
                .expect("idempotent retry")
                .expect("receipt")
                .idempotent_replay
        );
        drop(app);
        drop(store);
        let mut store = fixture.open();
        let restored = store.load_session(session_id).expect("restart");
        assert_eq!(
            restored.board.thought(thought_id).expect("durable").content,
            "Grüße 日本語\r\nwith e\u{301} and 👩🏽‍💻"
        );
        let mut app = BoardApp::new(
            AppState::from_snapshot(restored).expect("restored"),
            RopeEditorFactory,
        );
        if !board {
            input(&mut app, &mut ids, &clock, UiKey::Enter);
        }
        assert!(reflow(&mut app, &mut ids, &clock, board).is_empty());
        let effects = input(&mut app, &mut ids, &clock, UiKey::Undo);
        commit(&mut app, &mut store, &effects);
        assert_durable(&mut store, session_id, thought_id, source);
        let effects = input(&mut app, &mut ids, &clock, UiKey::Redo);
        commit(&mut app, &mut store, &effects);
        assert_durable(
            &mut store,
            session_id,
            thought_id,
            "Grüße 日本語\r\nwith e\u{301} and 👩🏽‍💻",
        );
    }
}

fn assert_durable(
    store: &mut SqliteStore,
    session_id: proqi::domain::SessionId,
    thought_id: proqi::domain::ThoughtId,
    expected: &str,
) {
    assert_eq!(
        store
            .load_session(session_id)
            .expect("durable snapshot")
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        expected
    );
}

fn busy_store(fixture: &DatabaseFixture) -> SqliteStore {
    let mut config = fixture.config.clone();
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter_seed: 0,
    };
    SqliteStore::open(&config).expect("store")
}
