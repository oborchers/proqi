//! Real v0.10.2 recovery followed by admitted upgrade and further exact recovery.

#[path = "recovery_upgrade/driver.rs"]
mod driver;

use proqi::{
    adapters::update::FileUpdateStateStore,
    domain::{InstanceId, StableVersion},
    ports::{runtime::InstanceInfo, update::UpdateStateStore as _},
};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use super::{
    active_instances,
    historical_fixture::{HistoricalFixture, HistoricalInstallation},
    old_fixture::{PROBE_TIMEOUT, run_bounded},
};
use crate::support::{json_command, json_input_command};
use driver::Owner;

const CONTENT: &str = "upgrade continuity Grüße 界\t\u{1b}[31m\u{7}\r\nretained history";

#[test]
fn historical_recovery_survives_fresh_upgrade_and_live_update_convergence() {
    let _fixture_guard = super::fixture_lock::acquire();
    let fixture = HistoricalFixture::build_recovery_release();
    for live in [false, true] {
        exercise_upgrade(&fixture, live);
    }
}

fn exercise_upgrade(fixture: &HistoricalFixture, live: bool) {
    let state = super::isolated_state("proqi-recovery-upgrade");
    let installation = fixture.install(state.path());
    let binary = installation.active_binary.to_str().expect("fixture binary");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session");
    json_input_command(binary, state.path(), &["thoughts", "add", session], CONTENT);
    json_command(
        binary,
        state.path(),
        &["sessions", "rename", session, "recovery fixture"],
    );
    seed_installation_observation(state.path(), &installation);
    let mut owner = Owner::spawn(
        &installation.active_binary,
        state.path(),
        session,
        "old-driver",
    );
    let before = wait_owner(state.path(), session, None);
    assert_eq!(before.version, "0.10.2");
    let old_healthy = recover(state.path(), session, &before);
    let path = recovery_path(state.path(), session);
    let old_record = fs::read(&path).expect("old healthy record");
    let old_lineage = record_state(state.path(), session)["lineage_id"].clone();
    assert_concurrent_refusal(binary, state.path(), session, &old_record);
    if !live {
        owner.stop();
    }
    installation.replace_externally();
    let mut fresh = None;
    let upgraded = if live {
        json_command(binary, state.path(), &["sessions", "list"]);
        let upgraded = wait_owner(state.path(), session, Some(old_healthy.instance_id));
        assert_eq!(upgraded.pid, old_healthy.pid, "verified update retains PID");
        let replacement = upgraded
            .update
            .as_ref()
            .and_then(|update| update.replacement.as_ref())
            .expect("update proof");
        assert_eq!(replacement.previous_instance_id, old_healthy.instance_id);
        upgraded
    } else {
        fresh = Some(Owner::spawn(
            &installation.active_binary,
            state.path(),
            session,
            "new-driver",
        ));
        wait_owner(state.path(), session, None)
    };
    assert_eq!(upgraded.version, env!("CARGO_PKG_VERSION"));
    assert!(!path.exists(), "admitted upgrade retires obsolete record");
    assert_cache_current(state.path(), &installation);
    assert_retirement_diagnostic(state.path(), true);
    assert_new_lineage_recovery(binary, state.path(), session, &upgraded, &old_lineage);
    let driver = fresh.as_mut().unwrap_or(&mut owner);
    driver.send(b"n\x1b[200~fresh accepted input\x1b[201~");
    wait_for_content(binary, state.path(), session);
    driver.refuse_next_stall(state.path());
    assert!(active_instances(state.path()).is_empty());
    assert_same_binary_resume(&installation, state.path(), session);
    assert_startup_refusals(&installation, state.path(), session);
}

fn assert_new_lineage_recovery(
    binary: &str,
    state: &Path,
    session: &str,
    upgraded: &InstanceInfo,
    old_lineage: &Value,
) {
    seed_current_metadata(binary, state, session);
    let snapshot = json_command(binary, state, &["thoughts", "list", session]);
    let counts = durable_counts(state);
    let after = recover(state, session, upgraded);
    assert_eq!(after.pid, upgraded.pid);
    let first_record = record_state(state, session);
    assert_ne!(&first_record["lineage_id"], old_lineage);
    assert_eq!(first_record["attempts"].as_array().map(Vec::len), Some(1));
    let retained = fs::read(recovery_path(state, session)).expect("current healthy record");
    assert_concurrent_refusal(binary, state, session, &retained);
    let _second = recover(state, session, &after);
    let second_record = record_state(state, session);
    assert_eq!(second_record["lineage_id"], first_record["lineage_id"]);
    assert_eq!(second_record["attempts"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        json_command(binary, state, &["thoughts", "list", session]),
        snapshot
    );
    assert_eq!(
        durable_counts(state),
        counts,
        "recovery never duplicates durable writes"
    );
    assert_stall_diagnostics(state, session);
}

fn record_state(state: &Path, session: &str) -> Value {
    serde_json::from_slice(&fs::read(recovery_path(state, session)).expect("recovery record"))
        .expect("record JSON")
}

fn recover(state: &Path, session: &str, before: &InstanceInfo) -> InstanceInfo {
    fs::write(state.join("runtime/input-stall-trigger"), b"stall").expect("arm one input incident");
    let after = wait_owner(state, session, Some(before.instance_id));
    assert_eq!(after.pid, before.pid);
    assert_eq!(after.session_id, before.session_id);
    assert_eq!(after.launch_directory, before.launch_directory);
    wait_until(
        || {
            fs::read(recovery_path(state, session))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|record| {
                    record["phase"]["stage"] == "healthy"
                        && record["phase"]["current_instance_id"] == after.instance_id.to_string()
                })
        },
        "real input progress must prove recovery healthy",
    );
    after
}

fn wait_owner(state: &Path, session: &str, previous: Option<InstanceId>) -> InstanceInfo {
    let mut found = None;
    wait_until(
        || {
            found = active_instances(state).into_iter().find(|owner| {
                owner.session_id.to_string() == session
                    && Some(owner.instance_id) != previous
                    && owner
                        .control_endpoint
                        .as_deref()
                        .is_some_and(|endpoint| Path::new(endpoint).exists())
            });
            found.is_some()
        },
        "exact ready owner",
    );
    found.expect("ready owner")
}

fn wait_until(mut condition: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !condition() {
        assert!(Instant::now() < deadline, "{label}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn recovery_path(state: &Path, session: &str) -> std::path::PathBuf {
    state
        .join("runtime/input-recovery")
        .join(format!("{session}.json"))
}

fn assert_concurrent_refusal(binary: &str, state: &Path, session: &str, record: &[u8]) {
    let mut command = Command::new(binary);
    command
        .arg("--state-dir")
        .arg(state)
        .args(["--json", "-r", session]);
    let output = run_bounded(&mut command, PROBE_TIMEOUT, "concurrent exact resume");
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
    assert_eq!(error["error"]["code"], "session_busy");
    assert_eq!(
        fs::read(recovery_path(state, session)).expect("owner record retained"),
        record
    );
}

fn seed_installation_observation(state: &Path, installation: &HistoricalInstallation) {
    FileUpdateStateStore::new(&state.join("cache"))
        .expect("cache")
        .record_restart_state(
            installation.identity,
            StableVersion::parse("0.10.2").expect("old version"),
            false,
        )
        .expect("old installation observation");
}

fn assert_cache_current(state: &Path, installation: &HistoricalInstallation) {
    let cache = FileUpdateStateStore::new(&state.join("cache"))
        .expect("cache")
        .load(installation.identity)
        .expect("cache state");
    assert_eq!(
        cache.observed_installed_version,
        Some(StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("current version"))
    );
    assert!(!cache.restart_needed);
    assert!(cache.external_restart.is_none());
}

fn seed_current_metadata(binary: &str, state: &Path, session: &str) {
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    let thought = thoughts["data"]["items"][0]["id"]
        .as_str()
        .expect("thought ID");
    assert_eq!(thoughts["data"]["items"][0]["content"], CONTENT);
    json_command(
        binary,
        state,
        &["thoughts", "rename", session, thought, "retained name"],
    );
    json_command(binary, state, &["items", "insert-separator", session]);
}

fn durable_counts(state: &Path) -> (i64, i64, i64, i64) {
    let connection =
        rusqlite::Connection::open(state.join("data/proqi.sqlite3")).expect("fixture database");
    connection.query_row("SELECT (SELECT count(*) FROM thoughts), (SELECT count(*) FROM separators), (SELECT count(*) FROM board_operations), (SELECT count(*) FROM thought_revisions)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).expect("durable counts")
}

fn wait_for_content(binary: &str, state: &Path, session: &str) {
    wait_until(
        || {
            let thoughts = json_command(binary, state, &["thoughts", "list", session]);
            thoughts["data"]["items"]
                .as_array()
                .expect("thoughts")
                .iter()
                .any(|thought| thought["content"] == "fresh accepted input")
        },
        "post-recovery PTY bytes must become durable",
    );
}

fn assert_same_binary_resume(installation: &HistoricalInstallation, state: &Path, session: &str) {
    let mut owner = Owner::spawn(
        &installation.active_binary,
        state,
        session,
        "same-binary-driver",
    );
    let before = wait_owner(state, session, None);
    assert!(!recovery_path(state, session).exists());
    assert_retirement_diagnostic(state, false);
    let _after = recover(state, session, &before);
    owner.stop();
    assert!(active_instances(state).is_empty());
}

fn assert_retirement_diagnostic(state: &Path, changed: bool) {
    assert!(
        events(state)
            .iter()
            .any(|event| event["event"] == "input_recovery_admission"
                && event["outcome"] == "retired"
                && event["retired_phase"] == "healthy"
                && event["executable_changed"] == changed)
    );
}

fn assert_stall_diagnostics(state: &Path, session: &str) {
    let evidence = events(state)
        .into_iter()
        .filter(|event| event["event"] == "input_stall")
        .collect::<Vec<_>>();
    assert!(!evidence.is_empty());
    for event in evidence {
        assert_eq!(event["outcome"], "confirmed");
        assert_eq!(event["reader_stage"], "poll");
        assert!(event["lease_gap_ms"].as_u64().is_some_and(|gap| gap >= 500));
        assert!(
            event["observer_gap_ms"]
                .as_u64()
                .is_some_and(|gap| gap < 500)
        );
        let encoded = event.to_string();
        assert!(!encoded.contains(CONTENT));
        assert!(!encoded.contains(session));
        assert!(!encoded.contains(state.to_str().expect("state path")));
        assert!(!encoded.contains("retained name"));
    }
}

fn events(state: &Path) -> Vec<Value> {
    super::diagnostic_content(state)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn assert_startup_refusals(installation: &HistoricalInstallation, state: &Path, session: &str) {
    let path = recovery_path(state, session);
    let record = fs::read(&path).expect("healthy record before refused startup");
    // A copy in an inactive owned keg must not reach recovery retirement.
    fs::copy(&installation.current_binary, &installation.old_binary)
        .expect("inactive synthetic keg");
    assert_startup_error(
        &installation.old_binary,
        state,
        session,
        "installation_failed",
    );
    assert_eq!(fs::read(&path).expect("retained record"), record);
    let mut newer = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("current version");
    newer.patch += 1;
    FileUpdateStateStore::new(&state.join("cache"))
        .expect("cache")
        .record_restart_state(
            installation.identity,
            StableVersion::parse(&newer.to_string()).expect("newer version"),
            false,
        )
        .expect("verified newer observation fixture");
    assert_startup_error(
        &installation.active_binary,
        state,
        session,
        "obsolete_executable",
    );
    assert_eq!(fs::read(path).expect("retained record"), record);
    assert!(active_instances(state).is_empty());
}

fn assert_startup_error(binary: &Path, state: &Path, session: &str, code: &str) {
    let mut command = Command::new(binary);
    command
        .arg("--state-dir")
        .arg(state)
        .args(["--json", "-r", session]);
    let output = run_bounded(&mut command, PROBE_TIMEOUT, "refused startup");
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
    assert_eq!(error["error"]["code"], code);
}
