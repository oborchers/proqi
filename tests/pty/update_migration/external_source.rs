//! Live source-install owners fail before external replacement quiescence.

use std::{path::Path, process::Command};

use proqi::{
    adapters::update::{FileUpdateStateStore, SystemInstallDetector},
    domain::{InstallationKind, StableVersion},
    ports::update::{InstallDetector as _, UpdateStateStore as _},
};

use super::{Owners, active_instances, isolated_state, rewrite_versions};

#[test]
fn source_install_owner_blocks_without_quiescence_or_cache_mutation() {
    let state = isolated_state("pq-src");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = super::json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let mut owners = Owners::spawn(binary, state.path(), std::slice::from_ref(&session));
    owners.wait_ready(state.path());
    let before = active_instances(state.path());
    assert_eq!(before.len(), 1);

    let installation = SystemInstallDetector::for_executable(binary.into())
        .detect()
        .expect("source installation");
    assert_eq!(installation.kind, InstallationKind::SourceOrUnknown);
    let observed = StableVersion::parse("0.9.0").expect("older observation");
    let cache = FileUpdateStateStore::new(&state.path().join("cache")).expect("update cache");
    cache
        .record_restart_state(installation.identity, observed.clone(), true)
        .expect("stale source observation");
    rewrite_versions(state.path(), "0.9.0");

    let (status, value) = run_json(Path::new(binary), state.path(), &["sessions", "list"]);
    assert!(
        !status.success(),
        "source owner must block external restart"
    );
    assert_eq!(value["error"]["code"], "external_upgrade_blocked");
    let blockers = value["error"]["details"]["blockers"]
        .as_array()
        .expect("exact blocker details");
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0]["session_id"], session);
    assert_eq!(blockers[0]["version"], "0.9.0");
    assert_eq!(blockers[0]["reason"], "restart_unsupported");

    owners.assert_running();
    let after = active_instances(state.path());
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].instance_id, before[0].instance_id);
    assert_eq!(after[0].session_id, before[0].session_id);
    let unchanged = cache.load(installation.identity).expect("unchanged cache");
    assert_eq!(unchanged.observed_installed_version, Some(observed));
    assert!(unchanged.restart_needed);
    assert!(unchanged.external_restart.is_none());
    owners.stop();
}

fn run_json(
    binary: &Path,
    state: &Path,
    arguments: &[&str],
) -> (std::process::ExitStatus, serde_json::Value) {
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run JSON command");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON output was invalid: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status, value)
}
