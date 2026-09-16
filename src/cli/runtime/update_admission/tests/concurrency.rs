use std::sync::{Arc, Barrier};

use super::*;

#[test]
fn concurrent_new_starters_follow_one_external_adoption() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let runtime_root = temporary.path().to_path_buf();
    let installation = InstallationIdentity::from_digest([96; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed, true)
        .expect("stale observation");
    let start = Arc::new(Barrier::new(8));
    let mut starters = Vec::new();
    for offset in 0..8_u64 {
        let cache = cache.clone();
        let runtime_root = runtime_root.clone();
        let current = current.clone();
        let start = Arc::clone(&start);
        starters.push(std::thread::spawn(move || {
            let mut ids = FakeIdGenerator::new(1_800_400_000_000 + offset * 10_000);
            let state = FileUpdateStateStore::new(&cache).expect("candidate update state");
            let snapshot = state.load(installation).expect("candidate observation");
            let candidate = snapshot
                .observed_installed_version
                .expect("stale candidate observation");
            let runtime = coordinator(
                &runtime_root,
                &mut ids,
                "0.10.0",
                installation,
                UPDATE_CONTROL_PROTOCOL_VERSION,
            );
            let installation = Installation {
                identity: installation,
                kind: InstallationKind::HomebrewFormula,
                executable: "/private/tmp/proqi".into(),
                restart_executable: Some("/private/tmp/proqi".into()),
            };
            let mut authority = SchemaOnlyAdoptionAuthority {
                coordinator: &runtime,
            };
            start.wait();
            let mut admission = reconcile_newer(
                &state,
                &runtime,
                &installation,
                None,
                &candidate,
                &current,
                None,
                &mut ids,
                Timestamp::from_millis(2),
                Duration::from_secs(2),
                &mut authority,
            )
            .map_err(|error| format!("admission: {error:?}"))?;
            admission
                .finish_after_store_ready()
                .map_err(|error| format!("reconciliation: {error:?}"))?;
            drop(admission);
            Ok::<(), String>(())
        }));
    }
    let failures = starters
        .into_iter()
        .filter_map(|starter| match starter.join() {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(format!("thread panic: {error:?}")),
        })
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "concurrent failures: {failures:?}");
    let reconciled = state.load(installation).expect("reconciled state");
    assert_eq!(reconciled.observed_installed_version, Some(current));
    assert!(!reconciled.restart_needed);
}
