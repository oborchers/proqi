//! Real on-disk SQLite contracts for commits, history, search, contention, and recovery.

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use proqi::{
    adapters::{
        memory::FakeIdGenerator,
        sqlite::{RetryPolicy, SqliteStore, StoreConfig},
    },
    application::{Action, AppState, Effect, reduce},
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, ContentAnnotation,
        ContentAnnotationKind, Direction, IntegrationContext, OperationSequence, Session,
        SessionBoard, TextPosition, ThoughtId, ThoughtPosition, ThoughtPresentation, Timestamp,
        UndoScope,
    },
    ports::{
        agent::{AgentState, SubmissionDisposition},
        environment::IdGenerator,
        store::{
            DurableIdentity, MigrationMode, OperationBatch, STORAGE_PROTOCOL_VERSION,
            SUPPORTED_SCHEMA_VERSION, SessionQuery, Store, StoreError, SubmissionAttempt,
            SubmissionAttemptState, SubmissionOutcome, SubmissionSource,
        },
    },
};
use rusqlite::Connection;

struct DatabaseFixture {
    _temporary: tempfile::TempDir,
    config: StoreConfig,
}

impl DatabaseFixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let config = StoreConfig::new(
            temporary.path().join("data/proqi.sqlite3"),
            temporary.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(100),
        );
        Self {
            _temporary: temporary,
            config,
        }
    }

    fn open(&self) -> SqliteStore {
        SqliteStore::open(&self.config).expect("open store")
    }
}

fn session_state(ids: &mut FakeIdGenerator, path: &Path) -> AppState {
    let session = Session::new(
        ids.session_id(),
        path.to_path_buf(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    AppState::new(SessionBoard::new(session, Vec::new()).expect("board"))
}

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn one_effect(state: &mut AppState, action: Action) -> Effect {
    let effects = reduce(state, action).expect("reduce");
    assert_eq!(effects.len(), 1);
    effects.into_iter().next().expect("effect")
}

fn persist_effect(store: &mut SqliteStore, effect: &Effect) -> proqi::ports::store::CommitReceipt {
    let batch = match effect {
        Effect::CommitBoardOperation(operation) => OperationBatch::Board(operation.clone()),
        Effect::CommitRevision(revision) => OperationBatch::Revision(revision.clone()),
        Effect::CommitHistoryMove {
            operation_id,
            session_id,
            scope,
            undo,
            sequence,
            at,
        } => OperationBatch::HistoryMove {
            operation_id: *operation_id,
            session_id: *session_id,
            scope: *scope,
            undo: *undo,
            sequence: *sequence,
            at: *at,
        },
        other => panic!("effect is not durable: {other:?}"),
    };
    store.commit(&batch).expect("commit").expect("receipt")
}

fn create_thought(
    store: &mut SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    content: &str,
    at: i64,
) -> ThoughtId {
    let thought_id = ids.thought_id();
    let effect = one_effect(
        state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(at),
        },
    );
    persist_effect(store, &effect);
    thought_id
}

#[path = "sqlite_store/bulk.rs"]
mod bulk;
#[path = "sqlite_store/compaction.rs"]
mod compaction;
#[path = "sqlite_store/concurrency.rs"]
mod concurrency;
#[path = "sqlite_store/core.rs"]
mod core;
#[path = "sqlite_store/editor.rs"]
mod editor;
#[path = "sqlite_store/recovery.rs"]
mod recovery;
#[path = "sqlite_store/screenshot.rs"]
mod screenshot;
#[path = "sqlite_store/submission.rs"]
mod submission;
#[path = "sqlite_store/transformations.rs"]
mod transformations;
