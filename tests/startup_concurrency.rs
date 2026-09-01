//! Real-process first-run schema coordination regressions.

use std::{
    collections::BTreeSet,
    path::Path,
    process::{Command, Output, Stdio},
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

use proqi::{
    adapters::{memory::FakeIdGenerator, runtime::FileRuntimeCoordinator},
    domain::Timestamp,
    ports::{
        environment::IdGenerator,
        runtime::RuntimeCoordinator,
        store::{STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION},
    },
};
use rusqlite::Connection;
use serde_json::Value;

const FRESH_PARTICIPANTS: usize = 5;
const FRESH_REPETITIONS: usize = 3;
const INITIALIZED_PARTICIPANTS: usize = 15;

fn launch(root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("launch Proqi")
}

fn launch_together(root: &Path, participants: usize) -> Vec<Output> {
    let barrier = Arc::new(Barrier::new(participants));
    std::thread::scope(|scope| {
        let workers = (0..participants)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    launch(root)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("launcher thread"))
            .collect()
    })
}

fn assert_successful_sessions(
    root: &Path,
    outputs: &[Output],
    expected_outputs: usize,
    expected_total: usize,
) {
    let sessions = outputs
        .iter()
        .map(|output| {
            assert!(
                output.status.success(),
                "startup failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let response: Value = serde_json::from_slice(&output.stdout).expect("startup JSON");
            response["data"]["session_id"]
                .as_str()
                .expect("session ID")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(sessions.len(), expected_outputs);
    assert_database(root, expected_total);
    assert_runtime_clean(root);
}

fn assert_database(root: &Path, expected_sessions: usize) {
    let connection = Connection::open(root.join("data/proqi.sqlite3")).expect("open database");
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .expect("quick check");
    let (schema, protocol): (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("schema metadata");
    let sessions: i64 = connection
        .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
        .expect("session count");
    assert_eq!(integrity, "ok");
    assert_eq!(schema, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(protocol, STORAGE_PROTOCOL_VERSION);
    assert_eq!(
        sessions,
        i64::try_from(expected_sessions).expect("session bound")
    );
}

fn assert_runtime_clean(root: &Path) {
    let instances = root.join("runtime/instances");
    let entries = std::fs::read_dir(instances)
        .expect("instance directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("instance entries");
    assert!(entries.is_empty(), "startup left instance metadata");
    assert_no_transient_runtime_artifacts(&root.join("runtime"));
}

fn assert_no_transient_runtime_artifacts(root: &Path) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("runtime directory") {
            let entry = entry.expect("runtime entry");
            let kind = entry.file_type().expect("runtime entry type");
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let path = entry.path();
            let transient = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    ["json", "tmp", "sock"]
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                });
            assert!(
                !transient,
                "startup left transient runtime artifact: {}",
                path.display()
            );
        }
    }
}

#[test]
fn concurrent_first_launches_coordinate_schema_initialization() {
    for _ in 0..FRESH_REPETITIONS {
        let state = tempfile::tempdir().expect("fresh state root");
        let outputs = launch_together(state.path(), FRESH_PARTICIPANTS);
        assert_successful_sessions(
            state.path(),
            &outputs,
            FRESH_PARTICIPANTS,
            FRESH_PARTICIPANTS,
        );
    }
}

#[test]
fn initialized_schema_accepts_fifteen_concurrent_processes() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    assert_successful_sessions(state.path(), &[initial], 1, 1);
    let outputs = launch_together(state.path(), INITIALIZED_PARTICIPANTS);
    assert_successful_sessions(
        state.path(),
        &outputs,
        INITIALIZED_PARTICIPANTS,
        INITIALIZED_PARTICIPANTS + 1,
    );
}

#[test]
fn bounded_schema_failure_leaves_no_runtime_advertisement_and_recovers() {
    let state = tempfile::tempdir().expect("state root");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let coordinator = FileRuntimeCoordinator::new(
        state.path().join("runtime"),
        ids.instance_id(),
        state.path().to_path_buf(),
        Timestamp::from_millis(1),
        "test-version",
    )
    .expect("coordinator");
    let exclusive = coordinator
        .acquire_schema_exclusive()
        .expect("exclusive schema holder");
    let started = Instant::now();
    let blocked = launch(state.path());
    assert!(!blocked.status.success());
    let response: Value = serde_json::from_slice(&blocked.stdout).expect("failure JSON");
    assert_eq!(response["error"]["code"], "schema_busy");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "schema contention exceeded its documented bound"
    );
    assert_runtime_clean(state.path());

    drop(exclusive);
    let recovered = launch(state.path());
    assert_successful_sessions(state.path(), &[recovered], 1, 1);
}

#[test]
fn shared_protocol_ten_owner_blocks_migration_without_backup_then_release_recovers() {
    let state = tempfile::tempdir().expect("state root");
    let initial = launch(state.path());
    assert_successful_sessions(state.path(), &[initial], 1, 1);
    let database = state.path().join("data/proqi.sqlite3");
    Connection::open(&database)
        .expect("protocol ten fixture")
        .execute_batch(
            "DELETE FROM migration_history WHERE version = 11;
             UPDATE schema_meta SET schema_version = 10, storage_protocol = 10;",
        )
        .expect("downgrade protocol stamp");

    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let coordinator = FileRuntimeCoordinator::new(
        state.path().join("runtime"),
        ids.instance_id(),
        state.path().to_path_buf(),
        Timestamp::from_millis(2),
        "protocol-ten-owner",
    )
    .expect("coordinator");
    let shared = coordinator
        .acquire_schema_shared()
        .expect("shared schema owner");
    let blocked = launch(state.path());
    assert!(!blocked.status.success());
    let response: Value = serde_json::from_slice(&blocked.stdout).expect("failure JSON");
    assert_eq!(response["error"]["code"], "schema_busy");
    let connection = Connection::open(&database).expect("unchanged protocol ten database");
    assert_eq!(
        connection
            .query_row(
                "SELECT schema_version, storage_protocol FROM schema_meta",
                [],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
            )
            .expect("versions"),
        (10, 10)
    );
    drop(connection);
    let backups = state.path().join("data/backups");
    assert!(
        !backups.exists()
            || std::fs::read_dir(&backups)
                .expect("backup directory")
                .next()
                .is_none()
    );

    drop(shared);
    let recovered = launch(state.path());
    assert_successful_sessions(state.path(), &[recovered], 1, 2);
    assert_eq!(
        std::fs::read_dir(backups)
            .expect("migration backup directory")
            .count(),
        1
    );
}
