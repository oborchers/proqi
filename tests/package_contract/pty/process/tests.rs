//! Absolute watchdog coverage for package PTY teardown.

use std::{
    env, fs,
    path::Path,
    process::{Child as ProcessChild, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use portable_pty::CommandBuilder;
use rustix::process::{
    Pid, Signal, kill_process, kill_process_group, test_kill_process, test_kill_process_group,
};

use super::{CleanupState, POLL_INTERVAL, PtyChild, TEARDOWN_TIMEOUT, TERM_GRACE, owner_is_ready};
use crate::{InstalledProduct, sandbox::PackageSandbox};

const HELPER_ENV: &str = "PROQI_PACKAGE_PTY_DROP_HELPER";
const TEST_NAME: &str =
    "pty::process::tests::drop_kills_term_resistant_descendant_before_absolute_deadline";
const SCENARIOS: [&str; 2] = ["drop", "repeat"];
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(12);

#[test]
fn readiness_requires_atomically_committed_owner_metadata() {
    let parent = tempfile::tempdir().expect("package parent");
    let sandbox = Arc::new(PackageSandbox::create(parent.path()).expect("package sandbox"));
    let product = InstalledProduct {
        binary: sandbox.root().join("unused-binary"),
        archive: sandbox.root().join("unused-archive"),
        state: sandbox.state().to_owned(),
        working: sandbox.working().to_owned(),
        sandbox,
    };
    let instances = product.state.join("runtime/instances");
    fs::create_dir_all(&instances).expect("runtime instances");
    let endpoint = product.state.join("runtime/control.sock");
    fs::write(&endpoint, []).expect("control endpoint fixture");
    let session = "ses_06g55re1ttq335mbmbihb1bag4";
    let metadata = serde_json::json!({
        "session_id": session,
        "control_protocol": proqi::ports::control::CONTROL_PROTOCOL_VERSION,
        "control_endpoint": endpoint,
    });
    let temporary = instances.join("ins_06g55re1vtv813nojkfu93pi9s.json.tmp");
    let committed = instances.join("ins_06g55re1vtv813nojkfu93pi9s.json");
    fs::write(
        &temporary,
        serde_json::to_vec(&metadata).expect("owner metadata"),
    )
    .expect("temporary owner metadata");
    assert!(
        !owner_is_ready(&product, session),
        "uncommitted owner metadata was treated as ready"
    );
    fs::rename(temporary, committed).expect("commit owner metadata");
    assert!(owner_is_ready(&product, session));
}

#[test]
fn reader_failure_remains_bounded_and_idempotent() {
    let reader = thread::Builder::new()
        .name("failing-package-pty-output".to_owned())
        .spawn(|| panic!("synthetic PTY reader failure"))
        .expect("spawn failing PTY reader");
    let deadline = Instant::now() + Duration::from_secs(1);
    while !reader.is_finished() && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
    assert!(reader.is_finished(), "failing PTY reader did not finish");
    let mut owner = PtyChild {
        child: None,
        process_group: None,
        input: None,
        output: Arc::new(Mutex::new(Vec::new())),
        reader: Some(reader),
        exit_status: None,
        cleanup: None,
        drop_success: None,
    };
    let first = owner.terminate();
    assert_eq!(first.reader, CleanupState::Failed);
    assert!(!first.successful());
    let repeated = Instant::now();
    let second = owner.terminate();
    assert_eq!(second, first, "reader failure became false cleanup success");
    assert!(repeated.elapsed() < Duration::from_millis(100));
}

#[test]
fn drop_kills_term_resistant_descendant_before_absolute_deadline() {
    if env::var_os(HELPER_ENV).is_some() {
        run_drop_helper();
        return;
    }
    let state = tempfile::tempdir().expect("PTY watchdog state");
    let stdout = state.path().join("helper.stdout");
    let stderr = state.path().join("helper.stderr");
    let started = Instant::now();
    let mut helper = Command::new(env::current_exe().expect("package contract test binary"));
    helper
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(HELPER_ENV, "1")
        .env("PROQI_PTY_WATCHDOG_STATE", state.path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            fs::File::create(&stdout).expect("create helper stdout"),
        ))
        .stderr(Stdio::from(
            fs::File::create(&stderr).expect("create helper stderr"),
        ));
    let mut helper = helper.spawn().expect("spawn PTY teardown watchdog helper");
    let deadline = Instant::now() + WATCHDOG_TIMEOUT;
    let Some(status) = status_before(&mut helper, deadline) else {
        cleanup_registered(state.path());
        let _ = helper.kill();
        assert!(
            status_before(&mut helper, Instant::now() + Duration::from_secs(1)).is_some(),
            "watchdog helper resisted SIGKILL"
        );
        assert_existing_registered_gone(state.path());
        panic!("PTY teardown exceeded its {WATCHDOG_TIMEOUT:?} absolute watchdog");
    };
    if !status.success() {
        let output = format!(
            "{}{}",
            fs::read_to_string(stdout).unwrap_or_default(),
            fs::read_to_string(stderr).unwrap_or_default()
        );
        cleanup_registered(state.path());
        assert_existing_registered_gone(state.path());
        panic!("PTY teardown helper exited {status}: {output}");
    }
    assert!(started.elapsed() < WATCHDOG_TIMEOUT);
    assert_all_registered_gone(state.path());
}

fn run_drop_helper() {
    let state = env::var_os("PROQI_PTY_WATCHDOG_STATE").expect("watchdog state");
    let state = Path::new(&state);
    let descendant_script = state.join("descendant.sh");
    fs::write(
        &descendant_script,
        r#"#!/bin/sh
trap '' HUP
trap '' TERM
printf '%s\n' "$$" > "$PROQI_DESCENDANT_PID"
while :; do /bin/sleep 1; done
"#,
    )
    .expect("write resistant PTY descendant");
    run_cleanup_scenario(state, &descendant_script, "drop", false);
    run_cleanup_scenario(state, &descendant_script, "repeat", true);
}

fn run_cleanup_scenario(state: &Path, script: &Path, name: &str, repeat: bool) {
    let descendant = state.join(format!("{name}-descendant.pid"));
    let group = state.join(format!("{name}-group.pid"));
    let mut command = CommandBuilder::new("/bin/sh");
    command.env_clear();
    command.args([
        "-c",
        r#"trap '' HUP
trap '' TERM
/bin/sh "$PROQI_DESCENDANT_SCRIPT" &
while test ! -s "$PROQI_DESCENDANT_PID"; do /bin/sleep 0.01; done
while :; do /bin/sleep 1; done"#,
    ]);
    command.env("PROQI_DESCENDANT_PID", &descendant);
    command.env("PROQI_DESCENDANT_SCRIPT", script);
    let mut owner = PtyChild::spawn_command(command);
    fs::write(&group, owner.process_id().to_string()).expect("record PTY process group");
    wait_for_file(&descendant, Instant::now() + Duration::from_secs(1));
    let drop_success = owner.observe_drop_success();
    let started = Instant::now();
    if repeat {
        let first = owner.terminate();
        assert!(first.successful(), "explicit PTY cleanup failed: {first:?}");
        let repeated = Instant::now();
        let second = owner.terminate();
        assert_eq!(second, first, "repeated cleanup changed its outcome");
        assert!(
            repeated.elapsed() < Duration::from_millis(100),
            "repeated cleanup did not reuse its completed outcome"
        );
    }
    drop(owner);
    let elapsed = started.elapsed();
    assert!(
        elapsed < TEARDOWN_TIMEOUT + Duration::from_millis(500),
        "PTY Drop exceeded its teardown deadline"
    );
    assert!(
        elapsed >= TERM_GRACE.saturating_sub(Duration::from_millis(25)),
        "PTY descendant did not retain the slave through the TERM grace period"
    );
    assert!(
        drop_success.load(Ordering::Acquire),
        "Drop returned without reaping the child and joining the PTY reader"
    );
    let group = read_pid(&group).expect("registered PTY group");
    let descendant = read_pid(&descendant).expect("registered PTY descendant");
    assert_pids_gone(Some(group), Some(descendant));
}

fn status_before(child: &mut ProcessChild, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait().expect("poll watchdog helper") {
            Some(status) => return Some(status),
            None if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            None => return None,
        }
    }
}

fn wait_for_file(path: &Path, deadline: Instant) {
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
    assert!(path.exists(), "PTY descendant did not register");
}

fn cleanup_registered(state: &Path) {
    for name in SCENARIOS {
        if let Some(group) = read_pid(&state.join(format!("{name}-group.pid"))) {
            let _ = kill_process_group(group, Signal::KILL);
        }
        if let Some(descendant) = read_pid(&state.join(format!("{name}-descendant.pid"))) {
            let _ = kill_process(descendant, Signal::KILL);
        }
    }
}

fn assert_all_registered_gone(state: &Path) {
    for name in SCENARIOS {
        let group =
            read_pid(&state.join(format!("{name}-group.pid"))).expect("registered PTY group");
        let descendant = read_pid(&state.join(format!("{name}-descendant.pid")))
            .expect("registered PTY descendant");
        assert_pids_gone(Some(group), Some(descendant));
    }
}

fn assert_existing_registered_gone(state: &Path) {
    for name in SCENARIOS {
        assert_pids_gone(
            read_pid(&state.join(format!("{name}-group.pid"))),
            read_pid(&state.join(format!("{name}-descendant.pid"))),
        );
    }
}

fn assert_pids_gone(group: Option<Pid>, descendant: Option<Pid>) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while (group.is_some_and(|group| !process_group_is_absent(group))
        || descendant.is_some_and(|descendant| !process_is_absent(descendant)))
        && Instant::now() < deadline
    {
        thread::sleep(POLL_INTERVAL);
    }
    assert!(
        group.is_none_or(process_group_is_absent),
        "PTY process group survived bounded teardown"
    );
    assert!(
        descendant.is_none_or(process_is_absent),
        "PTY descendant survived bounded teardown"
    );
}

fn process_group_is_absent(group: Pid) -> bool {
    matches!(test_kill_process_group(group), Err(rustix::io::Errno::SRCH))
}

fn process_is_absent(process: Pid) -> bool {
    matches!(test_kill_process(process), Err(rustix::io::Errno::SRCH))
}

fn read_pid(path: &Path) -> Option<Pid> {
    fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
        .and_then(Pid::from_raw)
}
