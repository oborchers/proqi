//! Driver lifetime and teardown, independent of migration product behavior.

use std::{fs, os::unix::fs::PermissionsExt as _, thread, time::Duration};

use super::cohort::Owners;

#[test]
fn unexpected_cohort_pty_exit_is_reported_before_coordinator_counters() {
    let root = tempfile::tempdir().expect("driver fixture");
    let mut owners = Owners::spawn("/bin/sleep", root.path(), &["synthetic".to_owned()]);
    owners.wait_started();
    let raw = fs::read_to_string(root.path().join("cohort.group")).expect("owned group");
    let group = rustix::process::Pid::from_raw(raw.trim().parse().expect("group PID"))
        .expect("positive group");
    rustix::process::kill_process_group(group, rustix::process::Signal::KILL)
        .expect("inject owned PTY closure");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        loop {
            owners.assert_running();
            thread::sleep(Duration::from_millis(20));
        }
    }));
    let failure = outcome.expect_err("driver exit must be detected");
    let message = failure.downcast_ref::<String>().expect("exit diagnostic");
    assert!(message.contains("exited before completion"), "{message}");
    assert_eq!(
        fs::read_to_string(root.path().join("cohort.exit")).expect("driver status"),
        "93"
    );
}

#[test]
fn cohort_watchdog_terminates_a_stuck_workflow_and_registered_owners() {
    let root = tempfile::tempdir().expect("driver fixture");
    let binary = root.path().join("owner");
    fs::write(
        &binary,
        "#!/bin/zsh -f\ntrap '' TERM\nwhile true; do read -t 1 ignored; done\n",
    )
    .expect("stuck owner");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    let mut owners = Owners::spawn_with_budget(
        binary.to_str().expect("path"),
        root.path(),
        &[("synthetic".to_owned(), root.path().to_path_buf())],
        Duration::from_secs(5),
    );
    let registration = owners.registration_path().to_path_buf();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        owners.wait_started();
        loop {
            owners.assert_running();
            thread::sleep(Duration::from_millis(20));
        }
    }));
    let failure = outcome.expect_err("watchdog must stop the unfinished workflow");
    let message = failure
        .downcast_ref::<String>()
        .expect("watchdog diagnostic");
    assert!(message.contains("absolute wall-clock limit"), "{message}");
    assert!(
        message.contains("driver: true, descendants: true, readers: true"),
        "{message}"
    );
    let pids = fs::read_to_string(registration).expect("registered owners");
    assert!(pids.lines().count() >= 2);
    for raw in pids.lines() {
        let pid = rustix::process::Pid::from_raw(raw.parse().expect("PID")).expect("positive PID");
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
    }
}

#[test]
fn successive_cohorts_do_not_reuse_retired_pid_registrations() {
    let root = tempfile::tempdir().expect("driver fixture");
    let binary = root.path().join("owner");
    fs::write(
        &binary,
        "#!/bin/zsh -f\ntrap 'exit 0' TERM\nprint ready > \"$PROQI_TEST_STATE/synthetic-ready\"\nwhile true; do read -t 1 ignored; done\n",
    )
    .expect("synthetic owner");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    let mut first = Owners::spawn(
        binary.to_str().expect("path"),
        root.path(),
        &["synthetic".to_owned()],
    );
    first.wait_started();
    super::super::support::wait_for_path(&root.path().join("synthetic-ready"));
    let retired = first.registration_path().to_path_buf();
    first.stop();
    fs::remove_file(root.path().join("synthetic-ready")).expect("reset synthetic readiness");
    assert!(!retired.exists(), "settled registration must be removed");
    // Simulate a stale file containing a live unrelated PID. The next watchdog
    // must neither read it nor signal that PID (the Rust test process).
    fs::write(&retired, std::process::id().to_string()).expect("retired PID sentinel");
    for signal in ["cohort.done", "cohort.group"] {
        fs::remove_file(root.path().join(signal)).expect("reset old cohort signal");
    }
    let mut second = Owners::spawn(
        binary.to_str().expect("path"),
        root.path(),
        &["synthetic".to_owned()],
    );
    assert_ne!(second.registration_path(), retired);
    second.wait_started();
    super::super::support::wait_for_path(&root.path().join("synthetic-ready"));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(second.registration_path())
        .expect("new registry")
        .lines()
        .count()
        < 2
    {
        assert!(
            std::time::Instant::now() < deadline,
            "new owner registration"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !fs::read_to_string(second.registration_path())
            .expect("new registry")
            .lines()
            .any(|pid| pid == std::process::id().to_string())
    );
    second.stop();
    assert_eq!(
        fs::read_to_string(retired).expect("untouched retired registry"),
        std::process::id().to_string()
    );
}

#[test]
fn cohort_driver_survives_the_former_twenty_second_cap() {
    let root = tempfile::tempdir().expect("driver fixture");
    let binary = root.path().join("owner");
    fs::write(
        &binary,
        "#!/bin/zsh -f\ntrap 'exit 0' TERM\nwhile true; do read -t 1 ignored; done\n",
    )
    .expect("synthetic owner");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    let mut owners = Owners::spawn(
        binary.to_str().expect("binary path"),
        root.path(),
        &["synthetic".to_owned()],
    );
    owners.wait_started();
    thread::sleep(Duration::from_secs(21));
    owners.assert_running();
    owners.stop();
}
