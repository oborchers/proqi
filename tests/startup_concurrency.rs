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
    domain::{SessionId, Timestamp},
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
const CHILD_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_TERMINATION_TIMEOUT: Duration = Duration::from_secs(2);
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(5);

struct BoundedChild {
    child: Option<Child>,
}

impl BoundedChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.as_mut().expect("live child").try_wait()
    }

    fn wait_with_output(mut self) -> Output {
        let deadline = Instant::now() + CHILD_WAIT_TIMEOUT;
        loop {
            if self.try_wait().expect("inspect startup process").is_some() {
                let child = self.child.take().expect("completed child");
                return child.wait_with_output().expect("reap startup");
            }
            assert!(
                Instant::now() < deadline,
                "startup process exceeded its bounded wait"
            );
            std::thread::sleep(CHILD_POLL_INTERVAL);
        }
    }

    fn terminate(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        let _kill = child.kill();
        let deadline = Instant::now() + CHILD_TERMINATION_TIMEOUT;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(CHILD_POLL_INTERVAL),
            }
        }
    }
}

impl Drop for BoundedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

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
    existing: &BTreeSet<String>,
) -> BTreeSet<String> {
    let launched = outputs
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
    assert_eq!(launched.len(), outputs.len());
    assert!(
        launched.is_disjoint(existing),
        "startup reused an existing session identity"
    );
    let sessions = existing.union(&launched).cloned().collect();
    assert_database(root, &sessions);
    assert_runtime_clean(root);
    sessions
}

fn assert_database(root: &Path, expected_sessions: &BTreeSet<String>) {
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
    let mut session_statement = connection
        .prepare("SELECT id FROM sessions ORDER BY id")
        .expect("prepare session IDs");
    let sessions = session_statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .expect("query session IDs")
        .map(|row| {
            let bytes: [u8; 16] = row
                .expect("session ID bytes")
                .try_into()
                .expect("session ID length");
            SessionId::from_database_bytes(bytes)
                .expect("valid session ID")
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    let mut search_statement = connection
        .prepare("SELECT session_id FROM session_search")
        .expect("prepare search IDs");
    let search_rows = search_statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query search IDs")
        .collect::<Result<Vec<_>, _>>()
        .expect("search ID rows");
    assert_eq!(integrity, "ok");
    assert_eq!(schema, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(protocol, STORAGE_PROTOCOL_VERSION);
    assert_eq!(&sessions, expected_sessions);
    assert_eq!(search_rows.len(), expected_sessions.len());
    assert_eq!(search_rows.into_iter().collect::<BTreeSet<_>>(), sessions);
    for table in [
        "thoughts",
        "board_operations",
        "thought_revisions",
        "commit_receipts",
        "integration_context",
        "submission_attempts",
        "screenshot_capture_receipts",
    ] {
        let rows: i64 = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("fresh-session side table");
        assert_eq!(rows, 0, "fresh sessions unexpectedly populated {table}");
    }
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

fn wait_for_runtime_advertisement(root: &Path, child: &mut BoundedChild) {
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
        std::thread::sleep(CHILD_POLL_INTERVAL);
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
        assert_successful_sessions(state.path(), &outputs, &BTreeSet::new());
    }
}

#[test]
fn initialized_schema_accepts_fifteen_concurrent_processes() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    let sessions = assert_successful_sessions(state.path(), &[initial], &BTreeSet::new());
    let outputs = launch_together(state.path(), INITIALIZED_PARTICIPANTS);
    assert_successful_sessions(state.path(), &outputs, &sessions);
}

#[test]
fn transient_writer_contention_recovers_without_duplication() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    let sessions = assert_successful_sessions(state.path(), &[initial], &BTreeSet::new());
    let writer =
        Connection::open(state.path().join("data/proqi.sqlite3")).expect("open writer connection");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("acquire writer lock");
    let mut child = BoundedChild::new(command(state.path()).spawn().expect("spawn startup"));
    wait_for_runtime_advertisement(state.path(), &mut child);
    std::thread::sleep(Duration::from_millis(300));
    writer.execute_batch("ROLLBACK").expect("release writer");

    let output = child.wait_with_output();
    assert_successful_sessions(state.path(), &[output], &sessions);
}

#[test]
fn persistent_writer_contention_is_bounded_and_leaves_no_session() {
    let state = tempfile::tempdir().expect("initialized state root");
    let initial = launch(state.path());
    let sessions = assert_successful_sessions(state.path(), &[initial], &BTreeSet::new());
    let writer =
        Connection::open(state.path().join("data/proqi.sqlite3")).expect("open writer connection");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("acquire writer lock");
    let mut blocked = BoundedChild::new(command(state.path()).spawn().expect("spawn startup"));
    wait_for_runtime_advertisement(state.path(), &mut blocked);
    let started = Instant::now();
    let blocked = blocked.wait_with_output();
    assert!(!blocked.status.success());
    let response: Value = serde_json::from_slice(&blocked.stdout).expect("failure JSON");
    assert_eq!(response["error"]["code"], "storage_busy");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "storage contention exceeded its bounded retry contract"
    );
    assert_database(state.path(), &sessions);
    assert_runtime_clean(state.path());

    writer.execute_batch("ROLLBACK").expect("release writer");
    let recovered = launch(state.path());
    assert_successful_sessions(state.path(), &[recovered], &sessions);
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
    assert_successful_sessions(state.path(), &[recovered], &BTreeSet::new());
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
