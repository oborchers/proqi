//! Bounded ownership and readiness checks for real replacement cohorts.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Child,
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

use super::super::support::{expect_command, wait_for_path};

pub(super) const OWNER_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Owners {
    child: Option<Child>,
    done: PathBuf,
    group: PathBuf,
    expected: usize,
}

impl Owners {
    pub(super) fn spawn(binary: &str, state: &Path, sessions: &[String]) -> Self {
        let done = state.join("cohort.done");
        let group = state.join("cohort.group");
        let child = spawn_owners(binary, state, sessions, &done, &group);
        Self {
            child: Some(child),
            done,
            group,
            expected: sessions.len(),
        }
    }

    pub(super) fn wait_started(&mut self) {
        wait_for_path(&self.group);
        self.assert_running();
    }

    pub(super) fn assert_running(&mut self) {
        assert!(
            self.child
                .as_mut()
                .expect("cohort owner")
                .try_wait()
                .expect("poll cohort owner")
                .is_none(),
            "replacement cohort exited before completion"
        );
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
                "owner did not become ready: {} active, {ready} control-ready, {} protocols, {} endpoints, {} existing endpoints, {} expected",
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
                self.expected
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn stop(mut self) {
        self.signal_done();
        let deadline = Instant::now() + OWNER_TIMEOUT;
        let child = self.child.as_mut().expect("cohort owner");
        loop {
            if let Some(status) = child.try_wait().expect("poll cohort shutdown") {
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
        self.child = None;
        assert!(!leaked_group, "cohort process group survived normal stop");
    }

    fn signal_done(&self) {
        let _written = fs::write(&self.done, b"done");
    }
}

impl Drop for Owners {
    fn drop(&mut self) {
        self.signal_done();
        if let Some(child) = &mut self.child {
            stop_child(child, read_process_group(&self.group));
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
            instance.control_protocol == Some(proqi::ports::control::CONTROL_PROTOCOL_VERSION)
                && Path::new(endpoint).exists()
        })
}

fn stop_child(child: &mut Child, group: Option<Pid>) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if let Some(group) = group {
        let _killed = kill_process_group(group, Signal::KILL);
    }
    if child.try_wait().is_ok_and(|status| status.is_none()) {
        let _killed = child.kill();
    }
    let _reaped = child.wait();
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

fn spawn_owners(
    binary: &str,
    state: &Path,
    sessions: &[String],
    done: &Path,
    group: &Path,
) -> Child {
    let script = r#"
        log_user 0
        set timeout 30
        spawn /bin/zsh -f -c {
            unsetopt BG_NICE MONITOR
            print $$ > "$PROQI_TEST_GROUP"
            exec {terminal_input}<&0
            typeset -a pids
            for session in ${(s: :)PROQI_TEST_SESSIONS}; do
                "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$session" <&$terminal_input &
                pids+=($!)
            done
            while [[ ! -e "$PROQI_TEST_DONE" ]]; do sleep 0.02; done
            kill -TERM $pids
            wait $pids
        }
        set deadline [expr {[clock milliseconds] + 20000}]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 93 }
            }
            after 20
        }
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSIONS", sessions.join(" "))
        .env("PROQI_TEST_DONE", done)
        .env("PROQI_TEST_GROUP", group)
        .spawn()
        .expect("spawn replacement owner")
}
