//! Real storage failure, retry, restart, and history for the UI's reflow action.

use super::{DatabaseFixture, key_input, session_state, test_path};
use proqi::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
        sqlite::{RetryPolicy, SqliteStore},
    },
    application::{AppState, Effect},
    domain::Timestamp,
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
    } else if cfg!(target_os = "macos") {
        LogicalModifiers::SUPER
    } else {
        LogicalModifiers::CONTROL
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
        let source = "Grüße 日本語\r\nwith e\u{301} and 👩🏽‍💻";
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
            "Grüße 日本語 with e\u{301} and 👩🏽‍💻"
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
            "Grüße 日本語 with e\u{301} and 👩🏽‍💻"
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
            "Grüße 日本語 with e\u{301} and 👩🏽‍💻",
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
