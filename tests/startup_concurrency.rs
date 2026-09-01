//! Real-process first-run schema coordination regressions.

use std::{
    collections::BTreeSet,
    path::Path,
    process::{Child, Command, Output, Stdio},
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

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_proqi"));
    command
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn launch(root: &Path) -> Output {
    command(root).output().expect("launch Proqi")
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

fn wait_for_runtime_advertisement(root: &Path, child: &mut Child) {
    let instances = root.join("runtime/instances");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let advertised =
            std::fs::read_dir(&instances).is_ok_and(|mut entries| entries.next().is_some());
        if advertised {
            return;
        }
        assert!(
            child.try_wait().expect("inspect startup process").is_none(),
            "startup exited before advertising its session lease"
        );
        assert!(
            Instant::now() < deadline,
            "startup did not advertise its session lease"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
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
fn transient_writer_contention_retries_one_session_creation_without_duplication() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    assert_successful_sessions(state.path(), &[initial], 1, 1);
    let writer =
        Connection::open(state.path().join("data/proqi.sqlite3")).expect("open writer connection");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("acquire writer lock");
    let mut child = command(state.path()).spawn().expect("spawn startup");
    wait_for_runtime_advertisement(state.path(), &mut child);
    std::thread::sleep(Duration::from_millis(300));
    writer.execute_batch("ROLLBACK").expect("release writer");

    let output = child.wait_with_output().expect("reap startup");
    assert_successful_sessions(state.path(), &[output], 1, 2);
}

#[test]
fn persistent_writer_contention_is_bounded_and_leaves_no_session() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    assert_successful_sessions(state.path(), &[initial], 1, 1);
    let writer =
        Connection::open(state.path().join("data/proqi.sqlite3")).expect("open writer connection");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("acquire writer lock");
    let started = Instant::now();
    let blocked = launch(state.path());
    assert!(!blocked.status.success());
    let response: Value = serde_json::from_slice(&blocked.stdout).expect("failure JSON");
    assert_eq!(response["error"]["code"], "storage_busy");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "storage contention exceeded its bounded retry contract"
    );
    assert_database(state.path(), 1);
    assert_runtime_clean(state.path());

    writer.execute_batch("ROLLBACK").expect("release writer");
    let recovered = launch(state.path());
    assert_successful_sessions(state.path(), &[recovered], 1, 2);
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
