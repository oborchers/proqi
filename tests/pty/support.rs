//! Shared process-boundary primitives for the PTY integration suite.
//!
//! This module owns Expect construction, bounded owner-readiness polling, and
//! JSON CLI invocation. Product scenarios and their behavioral assertions
//! belong in sibling modules.

use std::process::Command;

use serde_json::Value;

use proqi::{
    adapters::runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
    domain::SessionId,
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::{InstanceInfo, RuntimeCoordinator as _, RuntimeError},
    },
};

const OWNER_READINESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub(super) fn expect_command() -> Command {
    let mut command = Command::new("/usr/bin/expect");
    command
        .env("PROQI_DISABLE_HERDR", "1")
        .env("PROQI_TEST_PRIMARY_A", primary_sequence('a', 0x01))
        .env("PROQI_TEST_PRIMARY_Q", primary_sequence('q', 0x11))
        .env("PROQI_TEST_PRIMARY_Z", primary_sequence('z', 0x1a));
    command
}

fn primary_sequence(character: char, legacy_control: u8) -> String {
    if cfg!(target_os = "macos") {
        format!("\u{1b}[{};9u", u32::from(character))
    } else {
        char::from(legacy_control).to_string()
    }
}

pub(super) fn wait_for_path(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + OWNER_READINESS_TIMEOUT;
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub(super) fn wait_for_control_owner(state: &std::path::Path, session: &str) -> InstanceInfo {
    let session = session.parse::<SessionId>().expect("session ID");
    let mut ids = SystemIdGenerator;
    let coordinator = FileRuntimeCoordinator::new(
        state.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("runtime coordinator");
    let deadline = std::time::Instant::now() + OWNER_READINESS_TIMEOUT;
    loop {
        if let Some(participant) =
            ready_control_owner(&coordinator, session).expect("scan runtime owners")
        {
            return participant;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not advertise a ready control endpoint"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn ready_control_owner(
    coordinator: &FileRuntimeCoordinator,
    session: SessionId,
) -> Result<Option<InstanceInfo>, RuntimeError> {
    Ok(coordinator.active_instances()?.into_iter().find(|info| {
        info.session_id == session
            && info.control_protocol == Some(proqi::ports::control::CONTROL_PROTOCOL_VERSION)
            && info
                .control_endpoint
                .as_deref()
                .is_some_and(|endpoint| std::path::Path::new(endpoint).exists())
    }))
}

pub(super) fn raw_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> std::process::Output {
    use std::{io::Write as _, process::Stdio};

    let mut child = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn input command");
    child
        .stdin
        .take()
        .expect("command stdin")
        .write_all(input.as_bytes())
        .expect("write command input");
    child.wait_with_output().expect("wait for input command")
}

pub(super) fn json_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> Value {
    let output = raw_input_command(binary, state, arguments, input);
    assert!(
        output.status.success(),
        "input command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("input command JSON")
}

pub(super) fn json_command(binary: &str, state: &std::path::Path, arguments: &[&str]) -> Value {
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run JSON command");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

/// Launch and close one fresh interactive session to prepare later PTY scenarios.
pub(super) fn consume_first_run(binary: &str, state: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 400
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env_remove("HERDR_ENV")
        .status()
        .expect("close first interactive session");
    assert!(status.success());
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("first session ID");
    let _trashed = json_command(binary, state, &["sessions", "trash", session]);
    let _pruned = json_command(binary, state, &["sessions", "prune", session, "--yes"]);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proqi::{
        adapters::{
            memory::FakeIdGenerator,
            runtime::{FileRuntimeCoordinator, FileSessionLease},
        },
        domain::Timestamp,
        ports::{environment::IdGenerator as _, runtime::RuntimeCoordinator as _},
    };

    use super::ready_control_owner;

    struct OwnerFixture {
        lease: FileSessionLease,
        observer: FileRuntimeCoordinator,
        endpoint: PathBuf,
        runtime: PathBuf,
        _state: tempfile::TempDir,
    }

    fn owner_fixture(id_seed: u64) -> OwnerFixture {
        let state = tempfile::tempdir().expect("temporary state");
        let runtime = state.path().join("runtime");
        let mut ids = FakeIdGenerator::new(id_seed);
        let owner = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            state.path().to_path_buf(),
            Timestamp::from_millis(2),
            "1.0.0",
        )
        .expect("owner coordinator");
        let observer = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            state.path().to_path_buf(),
            Timestamp::from_millis(3),
            "1.0.0",
        )
        .expect("observer coordinator");
        let lease = owner
            .acquire_session(ids.session_id())
            .expect("session lease");
        let endpoint = PathBuf::from(lease.control_endpoint().expect("prepared endpoint"));
        std::fs::write(&endpoint, []).expect("endpoint fixture");
        OwnerFixture {
            lease,
            observer,
            endpoint,
            runtime,
            _state: state,
        }
    }

    #[test]
    fn readiness_returns_the_exact_canonical_owner_not_atomic_or_stale_neighbors() {
        let mut fixture = owner_fixture(1_800_100_000_000);
        let session = fixture.lease.info().session_id;
        let mut ready_temporary = fixture.lease.info().clone();
        ready_temporary.control_protocol = Some(proqi::ports::control::CONTROL_PROTOCOL_VERSION);
        ready_temporary.control_endpoint = Some(fixture.endpoint.to_string_lossy().into_owned());
        let temporary = fixture
            .runtime
            .join("instances")
            .join(format!("{}.json.tmp", ready_temporary.instance_id));
        std::fs::write(
            temporary,
            serde_json::to_vec(&ready_temporary).expect("temporary metadata"),
        )
        .expect("atomic-write neighbor");
        std::fs::write(fixture.runtime.join("instances/malformed.json"), b"{")
            .expect("malformed neighbor");

        let mut stale_ids = FakeIdGenerator::new(1_700_000_000_000);
        let mut stale = ready_temporary;
        stale.instance_id = stale_ids.instance_id();
        stale.started_at = Timestamp::from_millis(1);
        let stale_path = fixture
            .runtime
            .join("instances")
            .join(format!("{}.json", stale.instance_id));
        std::fs::write(
            stale_path,
            serde_json::to_vec(&stale).expect("stale metadata"),
        )
        .expect("stale same-session neighbor");

        assert_eq!(
            ready_control_owner(&fixture.observer, session).expect("initial scan"),
            None
        );
        fixture.lease.publish_control().expect("publish control");
        assert_eq!(
            ready_control_owner(&fixture.observer, session).expect("ready scan"),
            Some(fixture.lease.info().clone())
        );
    }

    #[test]
    fn readiness_requires_the_current_protocol_and_existing_endpoint() {
        let mut fixture = owner_fixture(1_800_200_000_000);
        let session = fixture.lease.info().session_id;
        fixture.lease.publish_control().expect("publish control");
        let metadata = fixture
            .runtime
            .join("instances")
            .join(format!("{}.json", fixture.lease.info().instance_id));
        let mut wrong_protocol = fixture.lease.info().clone();
        wrong_protocol.control_protocol =
            Some(proqi::ports::control::CONTROL_PROTOCOL_VERSION.saturating_sub(1));
        std::fs::write(
            &metadata,
            serde_json::to_vec(&wrong_protocol).expect("wrong protocol metadata"),
        )
        .expect("replace protocol fixture");
        assert_eq!(
            ready_control_owner(&fixture.observer, session).expect("protocol scan"),
            None
        );

        fixture.lease.publish_control().expect("restore protocol");
        std::fs::remove_file(&fixture.endpoint).expect("remove endpoint");
        assert_eq!(
            ready_control_owner(&fixture.observer, session).expect("endpoint scan"),
            None
        );
        std::fs::write(&fixture.endpoint, []).expect("restore endpoint");
        assert_eq!(
            ready_control_owner(&fixture.observer, session).expect("restored scan"),
            Some(fixture.lease.info().clone())
        );
    }
}
