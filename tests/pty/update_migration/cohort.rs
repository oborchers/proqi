//! Bounded ownership and readiness checks for real replacement cohorts.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use proqi::{
    adapters::runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::{InstanceInfo, RuntimeCoordinator as _},
    },
};
use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};

use super::super::{
    support::{expect_command, wait_for_path},
    watchdog::Workflow,
};

pub(super) const OWNER_TIMEOUT: Duration = Duration::from_secs(15);
// Startup, two final readiness checks, normal shutdown, and the complete
// historical coordinator (including its pre-finish installer/probe stages).
const COHORT_TIMEOUT: Duration = OWNER_TIMEOUT
    .saturating_mul(4)
    .saturating_add(super::old_fixture::COORDINATOR_TIMEOUT)
    .saturating_add(super::old_fixture::PROBE_TIMEOUT.saturating_mul(2));

pub(super) struct Owners {
    workflow: Option<Workflow>,
    registration: tempfile::NamedTempFile,
    done: PathBuf,
    group: PathBuf,
    expected: usize,
    started: Instant,
}

impl Owners {
    pub(super) fn spawn(binary: &str, state: &Path, sessions: &[String]) -> Self {
        let cwd = std::env::current_dir().expect("cohort working directory");
        let launches = sessions
            .iter()
            .cloned()
            .map(|session| (session, cwd.clone()))
            .collect::<Vec<_>>();
        Self::spawn_in_directories(binary, state, &launches)
    }

    pub(super) fn spawn_in_directories(
        binary: &str,
        state: &Path,
        launches: &[(String, PathBuf)],
    ) -> Self {
        Self::spawn_with_budget(binary, state, launches, COHORT_TIMEOUT)
    }

    pub(super) fn spawn_with_budget(
        binary: &str,
        state: &Path,
        launches: &[(String, PathBuf)],
        budget: Duration,
    ) -> Self {
        let done = state.join("cohort.done");
        let group = state.join("cohort.group");
        Self::start_driver(binary, state, launches, done, group, budget)
    }

    fn start_driver(
        binary: &str,
        state: &Path,
        launches: &[(String, PathBuf)],
        done: PathBuf,
        group: PathBuf,
        budget: Duration,
    ) -> Self {
        let registration = tempfile::Builder::new()
            .prefix("cohort-pids-")
            .tempfile_in(state)
            .expect("fresh cohort registration");
        let command = owner_command(binary, state, launches, &done, &group, registration.path());
        let workflow = Workflow::spawn(
            command,
            budget,
            registration.path().to_path_buf(),
            "replacement cohort driver",
        );
        Self {
            workflow: Some(workflow),
            registration,
            done,
            group,
            expected: launches.len(),
            started: Instant::now(),
        }
    }

    pub(super) fn wait_started(&mut self) {
        wait_for_path(&self.group);
        self.assert_running();
    }

    pub(super) fn registration_path(&self) -> &Path {
        self.registration.path()
    }

    pub(super) fn assert_running(&mut self) {
        let workflow = self.workflow.as_mut().expect("cohort owner");
        if workflow.is_finished() {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| workflow.finish()));
            let status = match outcome {
                Ok(status) => status,
                Err(panic) => {
                    let _recorded = fs::write(self.done.with_file_name("cohort.exit"), "-2");
                    std::panic::resume_unwind(panic);
                }
            };
            fs::write(
                self.done.with_file_name("cohort.exit"),
                status.code().unwrap_or(-1).to_string(),
            )
            .expect("record driver exit");
            panic!(
                "replacement cohort exited before completion: {status}; elapsed={:?}",
                self.started.elapsed()
            );
        }
    }

    pub(super) fn wait_ready(&mut self, state: &Path) {
        let deadline = Instant::now() + OWNER_TIMEOUT;
        loop {
            let active = active_instances(state);
            let ready = active
                .iter()
                .filter(|instance| control_ready(instance))
                .count();
            if ready == self.expected {
                return;
            }
            self.assert_running();
            assert!(
                Instant::now() < deadline,
                "owner did not become ready: {} active, {ready} control-ready, {} protocols, {} endpoints, {} existing endpoints, {} expected; diagnostics: {}",
                active.len(),
                active
                    .iter()
                    .filter(|item| item.control_protocol.is_some())
                    .count(),
                active
                    .iter()
                    .filter(|item| item.control_endpoint.is_some())
                    .count(),
                active
                    .iter()
                    .filter(|item| item
                        .control_endpoint
                        .as_deref()
                        .is_some_and(|endpoint| Path::new(endpoint).exists()))
                    .count(),
                self.expected,
                super::diagnostic_content(state)
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn stop(mut self) {
        self.signal_done();
        let deadline = Instant::now() + OWNER_TIMEOUT;
        let workflow = self.workflow.as_mut().expect("cohort owner");
        loop {
            if workflow.is_finished() {
                let status = workflow.finish();
                assert!(status.success(), "cohort owner exited with {status}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cohort shutdown exceeded its bound"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let leaked_group = !process_group_absent(&self.group);
        if leaked_group {
            kill_recorded_process_group(&self.group);
        }
        self.workflow = None;
        assert!(!leaked_group, "cohort process group survived normal stop");
    }

    fn signal_done(&self) {
        let _written = fs::write(&self.done, b"done");
    }
}

impl Drop for Owners {
    fn drop(&mut self) {
        self.signal_done();
        if self.workflow.is_some() {
            kill_recorded_process_group(&self.group);
        }
    }
}

pub(super) fn active_instances(state: &Path) -> Vec<InstanceInfo> {
    let mut ids = SystemIdGenerator;
    FileRuntimeCoordinator::new(
        state.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("working directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("runtime scan")
    .active_instances()
    .expect("active instances")
}

pub(super) fn control_ready(instance: &InstanceInfo) -> bool {
    instance
        .control_endpoint
        .as_deref()
        .is_some_and(|endpoint| {
            proqi::ports::control::control_protocol_supports(
                instance.control_protocol,
                proqi::ports::control::UPDATE_MUTATION_MINIMUM_PROTOCOL,
            ) && Path::new(endpoint).exists()
        })
}

fn kill_recorded_process_group(path: &Path) {
    if let Some(group) = read_process_group(path) {
        let _killed = kill_process_group(group, Signal::KILL);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !process_group_absent(path) {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn read_process_group(path: &Path) -> Option<Pid> {
    fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .and_then(Pid::from_raw)
}

fn process_group_absent(path: &Path) -> bool {
    read_process_group(path)
        .is_none_or(|group| matches!(test_kill_process_group(group), Err(rustix::io::Errno::SRCH)))
}

fn owner_command(
    binary: &str,
    state: &Path,
    launches: &[(String, PathBuf)],
    done: &Path,
    group: &Path,
    pids: &Path,
) -> Command {
    let script = r#"
        log_user 0
        set timeout 30
        spawn /bin/zsh -f -c {
            unsetopt BG_NICE MONITOR
            print $$ > "$PROQI_TEST_GROUP"
            print $$ >> "$PROQI_TEST_PIDS"
            exec {terminal_input}<&0
            typeset -a pids
            typeset -a sessions=(${(s: :)PROQI_TEST_SESSIONS})
            typeset -a directories=(${(s: :)PROQI_TEST_DIRECTORIES})
            for (( index = 1; index <= ${#sessions}; index++ )); do
                session=$sessions[$index]
                directory=$directories[$index]
                (cd "$directory" && exec "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$session" <&$terminal_input) &
                pids+=($!)
                print $! >> "$PROQI_TEST_PIDS"
            done
            while [[ ! -e "$PROQI_TEST_DONE" ]]; do sleep 0.02; done
            kill -TERM $pids
            wait $pids
        }
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[catch {expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 93 }
            }}]} { exit 93 }
            after 20
        }
        if {[catch {expect eof}]} { exit 94 }
        catch wait result
        exit [lindex $result 3]
    "#;
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env(
            "PROQI_TEST_SESSIONS",
            launches
                .iter()
                .map(|(session, _)| session.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        )
        .env(
            "PROQI_TEST_DIRECTORIES",
            launches
                .iter()
                .map(|(_, directory)| directory.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
        )
        .env("PROQI_TEST_DONE", done)
        .env("PROQI_TEST_GROUP", group)
        .env("PROQI_TEST_PIDS", pids);
    command
}
