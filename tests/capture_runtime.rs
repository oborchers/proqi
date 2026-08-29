//! Installation-wide screenshot-capture ownership and crash release.

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use proqi::{
    adapters::{memory::FakeIdGenerator, runtime::FileRuntimeCoordinator},
    domain::{InstanceId, SessionId, Timestamp},
    ports::{
        environment::IdGenerator,
        runtime::{CaptureCoordinator, CaptureLease, CaptureLockError, RuntimeCoordinator},
    },
};

const CHILD_LOCK_READY: &str = "PROQI_CAPTURE_LOCK_READY";
const CHILD_RUNTIME_ENV: &str = "PROQI_TEST_CAPTURE_RUNTIME";
const CHILD_INSTANCE_ENV: &str = "PROQI_TEST_CAPTURE_INSTANCE";
const CHILD_SESSION_ENV: &str = "PROQI_TEST_CAPTURE_SESSION";

fn coordinator(runtime: PathBuf, instance_id: InstanceId, started: i64) -> FileRuntimeCoordinator {
    FileRuntimeCoordinator::new(
        runtime,
        instance_id,
        std::env::temp_dir().join("proqi-capture-runtime-contract"),
        Timestamp::from_millis(started),
        "test-version",
    )
    .expect("coordinator")
}

#[test]
fn screenshot_capture_lock_is_installation_wide_and_separate_from_session_leases() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_250_000_000);
    let first = coordinator(runtime.clone(), ids.instance_id(), 1);
    let second = coordinator(runtime, ids.instance_id(), 2);
    let mut first_session = first
        .acquire_session(ids.session_id())
        .expect("first session");
    first_session.publish_control().expect("first control");
    let mut second_session = second
        .acquire_session(ids.session_id())
        .expect("second session");
    second_session.publish_control().expect("second control");

    let first_capture = first
        .acquire_capture(first_session.info())
        .expect("first capture");
    assert_eq!(
        first_capture.owner().instance_id,
        first_session.info().instance_id
    );
    let error = second
        .acquire_capture(second_session.info())
        .expect_err("capture must be exclusive");
    assert!(matches!(
        error,
        CaptureLockError::Busy { owner: Some(owner) }
            if owner.instance_id == first_session.info().instance_id
                && owner.session_id == first_session.info().session_id
    ));

    drop(first_capture);
    second
        .acquire_capture(second_session.info())
        .expect("capture released on drop");
}

#[test]
#[ignore = "child fixture, driven by process_termination_releases_capture_lock"]
fn child_process_holds_capture_lock() {
    let Ok(runtime) = std::env::var(CHILD_RUNTIME_ENV) else {
        return;
    };
    let session = SessionId::from_str(&std::env::var(CHILD_SESSION_ENV).expect("child session"))
        .expect("session ID");
    let instance =
        InstanceId::from_str(&std::env::var(CHILD_INSTANCE_ENV).expect("child instance"))
            .expect("instance ID");
    let owner = coordinator(PathBuf::from(runtime), instance, 1);
    let mut session_lease = owner.acquire_session(session).expect("child session lease");
    session_lease.publish_control().expect("child control");
    let _capture = owner
        .acquire_capture(session_lease.info())
        .expect("child capture lease");
    println!("{CHILD_LOCK_READY}");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn process_termination_releases_capture_lock() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let mut ids = FakeIdGenerator::new(1_725_251_000_000);
    let child_session = ids.session_id();
    let child_instance = ids.instance_id();
    let parent = coordinator(runtime.clone(), ids.instance_id(), 2);
    let mut parent_session = parent
        .acquire_session(ids.session_id())
        .expect("parent session");
    parent_session.publish_control().expect("parent control");
    let executable = std::env::current_exe().expect("test executable");
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("child_process_holds_capture_lock")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_RUNTIME_ENV, &runtime)
        .env(CHILD_SESSION_ENV, child_session.to_string())
        .env(CHILD_INSTANCE_ENV, child_instance.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    wait_until_child_holds_lock(&mut child);
    assert!(matches!(
        parent.acquire_capture(parent_session.info()),
        Err(CaptureLockError::Busy { .. })
    ));
    child.kill().expect("terminate child");
    child.wait().expect("reap child");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match parent.acquire_capture(parent_session.info()) {
            Ok(_lease) => break,
            Err(CaptureLockError::Busy { .. }) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            result => panic!("capture lock was not released after crash: {result:?}"),
        }
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
