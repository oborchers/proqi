//! File-lock ownership, schema exclusion, stale metadata, and crash release contracts.

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use proqi::{
    adapters::{
        memory::FakeIdGenerator,
        runtime::{FileRuntimeCoordinator, NativePaths, SchemaLockPolicy},
    },
    domain::{InstanceId, SessionId, Timestamp},
    ports::{
        environment::{IdGenerator, Paths},
        runtime::{RuntimeCoordinator, RuntimeError},
    },
};

const CHILD_LOCK_READY: &str = "PROQI_LOCK_READY";
const CHILD_RUNTIME_ENV: &str = "PROQI_TEST_CHILD_RUNTIME";
const CHILD_INSTANCE_ENV: &str = "PROQI_TEST_CHILD_INSTANCE";
const CHILD_SESSION_ENV: &str = "PROQI_TEST_CHILD_SESSION";

fn coordinator(runtime: PathBuf, instance_id: InstanceId, started: i64) -> FileRuntimeCoordinator {
    FileRuntimeCoordinator::new(
        runtime,
        instance_id,
        std::env::temp_dir().join("proqi-runtime-contract"),
        Timestamp::from_millis(started),
        "test-version",
    )
    .expect("coordinator")
}

fn coordinator_with_schema_wait(
    runtime: PathBuf,
    instance_id: InstanceId,
    started: i64,
    timeout: Duration,
) -> FileRuntimeCoordinator {
    coordinator(runtime, instance_id, started).with_schema_lock_policy(
        SchemaLockPolicy::new(timeout, Duration::from_millis(2)).expect("schema policy"),
    )
}

#[test]
fn native_paths_are_absolute() {
    let paths = NativePaths.resolve().expect("native paths");
    assert!(paths.data_dir.is_absolute());
    assert!(paths.config_dir.is_absolute());
    assert!(paths.runtime_dir.is_absolute());
}

#[test]
fn one_session_has_one_authoritative_owner() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = ids.session_id();
    let first = coordinator(runtime.clone(), ids.instance_id(), 1);
    let second = coordinator(runtime, ids.instance_id(), 2);

    let lease = first.acquire_session(session_id).expect("first lease");
    let error = second
        .acquire_session(session_id)
        .expect_err("must conflict");
    assert!(matches!(
        error,
        RuntimeError::SessionBusy {
            session_id: busy,
            holder: Some(holder),
        } if busy == session_id && holder.instance_id == lease.info().instance_id
    ));
    assert_eq!(first.active_instances().expect("active").len(), 1);

    drop(lease);
    assert!(first.active_instances().expect("inactive").is_empty());
    second.acquire_session(session_id).expect("released lease");
}

#[test]
fn different_sessions_and_shared_schema_leases_can_coexist() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let timeout = Duration::from_millis(20);
    let first = coordinator_with_schema_wait(runtime.clone(), ids.instance_id(), 1, timeout);
    let second = coordinator_with_schema_wait(runtime, ids.instance_id(), 2, timeout);
    let _first_session = first
        .acquire_session(ids.session_id())
        .expect("first session");
    let _second_session = second
        .acquire_session(ids.session_id())
        .expect("second session");
    let first_schema = first.acquire_schema_shared().expect("first shared");
    let second_schema = second.acquire_schema_shared().expect("second shared");
    assert!(matches!(
        first.acquire_schema_exclusive(),
        Err(RuntimeError::SchemaBusy)
    ));
    drop(first_schema);
    drop(second_schema);
    let _exclusive = first.acquire_schema_exclusive().expect("exclusive");
    assert!(matches!(
        second.acquire_schema_shared(),
        Err(RuntimeError::SchemaBusy)
    ));
}

#[test]
fn schema_contender_waits_for_a_brief_holder_and_times_out_when_unavailable() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let holder = coordinator_with_schema_wait(
        runtime.clone(),
        ids.instance_id(),
        1,
        Duration::from_millis(250),
    );
    let contender = coordinator_with_schema_wait(
        runtime.clone(),
        ids.instance_id(),
        2,
        Duration::from_millis(250),
    );
    let shared = holder.acquire_schema_shared().expect("shared lease");
    let worker = thread::spawn(move || contender.acquire_schema_exclusive());
    thread::sleep(Duration::from_millis(30));
    drop(shared);
    let exclusive = worker.join().expect("contender thread").expect("exclusive");

    let bounded =
        coordinator_with_schema_wait(runtime, ids.instance_id(), 3, Duration::from_millis(40));
    let started = Instant::now();
    assert!(matches!(
        bounded.acquire_schema_shared(),
        Err(RuntimeError::SchemaBusy)
    ));
    assert!(started.elapsed() >= Duration::from_millis(35));
    drop(exclusive);
}

#[test]
fn stale_descriptive_metadata_is_removed_when_no_lock_exists() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = ids.session_id();
    let instance_id = ids.instance_id();
    let owner = coordinator(runtime.clone(), instance_id, 1);
    let mut lease = owner.acquire_session(session_id).expect("lease");
    lease.publish_control().expect("publish control");
    let info = lease.info().clone();
    let metadata = runtime
        .join("instances")
        .join(format!("{instance_id}.json"));
    assert!(metadata.exists());
    drop(lease);
    assert!(!metadata.exists());

    #[cfg(unix)]
    let stale_endpoint = {
        let endpoint = PathBuf::from(info.control_endpoint.as_deref().expect("control endpoint"));
        std::fs::write(&endpoint, b"stale").expect("stale endpoint fixture");
        endpoint
    };
    std::fs::write(&metadata, serde_json::to_vec(&info).expect("json"))
        .expect("stale metadata fixture");
    let scan = owner.scan_runtime().expect("recovery");
    assert!(scan.active.is_empty());
    assert_eq!(scan.recovered, vec![session_id]);
    assert!(!metadata.exists());
    #[cfg(unix)]
    assert!(!stale_endpoint.exists());

    #[cfg(unix)]
    {
        let unrelated = temporary.path().join("unrelated.sock");
        std::fs::write(&unrelated, b"keep").expect("unrelated endpoint fixture");
        let mut untrusted = info.clone();
        untrusted.control_endpoint = Some(unrelated.to_string_lossy().into_owned());
        std::fs::write(&metadata, serde_json::to_vec(&untrusted).expect("json"))
            .expect("untrusted metadata fixture");
        owner.scan_runtime().expect("safe recovery");
        assert!(unrelated.exists());
    }

    #[cfg(unix)]
    {
        let endpoint = PathBuf::from(info.control_endpoint.as_deref().expect("control endpoint"));
        std::fs::write(&endpoint, b"stale").expect("resume endpoint fixture");
        std::fs::write(&metadata, serde_json::to_vec(&info).expect("json"))
            .expect("resume metadata fixture");
        let resumed = owner.acquire_session(session_id).expect("direct recovery");
        assert!(!endpoint.exists());
        drop(resumed);
    }

    let malformed = runtime.join("instances").join("malformed.json");
    std::fs::write(&malformed, b"{").expect("malformed metadata fixture");
    let _lease = owner
        .acquire_session(session_id)
        .expect("metadata is not authority");
}

#[test]
#[ignore = "child fixture, driven by process_termination_releases_authoritative_lock"]
fn child_process_holds_session_lock() {
    let Ok(runtime) = std::env::var(CHILD_RUNTIME_ENV) else {
        return;
    };
    let session = SessionId::from_str(&std::env::var(CHILD_SESSION_ENV).expect("child session"))
        .expect("session ID");
    let instance =
        InstanceId::from_str(&std::env::var(CHILD_INSTANCE_ENV).expect("child instance"))
            .expect("instance ID");
    let owner = coordinator(PathBuf::from(runtime), instance, 1);
    let _lease = owner.acquire_session(session).expect("child lease");
    println!("{CHILD_LOCK_READY}");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
#[ignore = "child fixture, driven by process_termination_releases_schema_lock"]
fn child_process_holds_schema_lock() {
    let Ok(runtime) = std::env::var(CHILD_RUNTIME_ENV) else {
        return;
    };
    let instance =
        InstanceId::from_str(&std::env::var(CHILD_INSTANCE_ENV).expect("child instance"))
            .expect("instance ID");
    let owner = coordinator(PathBuf::from(runtime), instance, 1);
    let _lease = owner.acquire_schema_exclusive().expect("schema lease");
    println!("{CHILD_LOCK_READY}");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn wait_until_child_holds_lock(child: &mut std::process::Child) {
    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("child output");
        assert!(read > 0, "child exited before acquiring the lock");
        if line.contains(CHILD_LOCK_READY) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "child did not acquire lock in time"
        );
    }
}

#[test]
fn process_termination_releases_authoritative_lock() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = ids.session_id();
    let child_instance = ids.instance_id();
    let parent = coordinator(runtime.clone(), ids.instance_id(), 2);
    let executable = std::env::current_exe().expect("test executable");
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("child_process_holds_session_lock")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_RUNTIME_ENV, &runtime)
        .env(CHILD_SESSION_ENV, session_id.to_string())
        .env(CHILD_INSTANCE_ENV, child_instance.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    wait_until_child_holds_lock(&mut child);
    assert!(matches!(
        parent.acquire_session(session_id),
        Err(RuntimeError::SessionBusy { .. })
    ));
    child.kill().expect("terminate child");
    child.wait().expect("reap child");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match parent.acquire_session(session_id) {
            Ok(_lease) => break,
            Err(RuntimeError::SessionBusy { .. }) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20));
            }
            result => panic!("lock was not released after process termination: {result:?}"),
        }
    }
}

#[test]
fn process_termination_releases_schema_lock() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let child_instance = ids.instance_id();
    let parent = coordinator_with_schema_wait(
        runtime.clone(),
        ids.instance_id(),
        2,
        Duration::from_millis(40),
    );
    let executable = std::env::current_exe().expect("test executable");
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("child_process_holds_schema_lock")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_RUNTIME_ENV, &runtime)
        .env(CHILD_INSTANCE_ENV, child_instance.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    wait_until_child_holds_lock(&mut child);

    assert!(matches!(
        parent.acquire_schema_shared(),
        Err(RuntimeError::SchemaBusy)
    ));
    child.kill().expect("terminate child");
    child.wait().expect("reap child");

    parent
        .acquire_schema_shared()
        .expect("schema lock released after child termination");
}

#[cfg(unix)]
#[test]
fn runtime_files_are_user_only() {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let owner = coordinator(runtime.clone(), ids.instance_id(), 1);
    let lease = owner.acquire_session(ids.session_id()).expect("lease");
    assert_eq!(
        std::fs::metadata(&runtime)
            .expect("runtime metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let metadata = runtime
        .join("instances")
        .join(format!("{}.json", lease.info().instance_id));
    assert_eq!(
        std::fs::metadata(metadata)
            .expect("instance metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(lease.info().control_protocol, None);
    assert_eq!(lease.info().control_endpoint, None);
    let endpoint = lease.control_endpoint().expect("prepared endpoint");
    assert!(
        endpoint.len() < 100,
        "Unix socket path is bounded: {endpoint}"
    );
    assert_eq!(
        std::fs::metadata(
            std::path::Path::new(endpoint)
                .parent()
                .expect("control parent")
        )
        .expect("control directory")
        .permissions()
        .mode()
            & 0o777,
        0o700
    );
}
