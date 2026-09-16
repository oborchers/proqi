use super::*;

#[test]
fn hidden_same_schema_writer_blocks_external_adoption_until_exclusive_quiescence() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([98; 32]);
    let observed = StableVersion::parse("0.10.0").expect("observed version");
    let current = StableVersion::parse("0.11.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("stale observation");
    let mut ids = FakeIdGenerator::new(1_800_350_000_000);
    let old = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    let old_owner = old.acquire_session(ids.session_id()).expect("old owner");
    let old_schema = old
        .acquire_schema_shared()
        .expect("same-schema writer lease");
    let metadata = temporary
        .path()
        .join("runtime/instances")
        .join(format!("{}.json", old_owner.info().instance_id));
    fs::remove_file(&metadata).expect("remove descriptive runtime metadata");
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.11.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let Err(blocked) = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(2),
        Duration::ZERO,
    ) else {
        panic!("a hidden same-schema writer must prevent external adoption");
    };
    assert!(format!("{blocked:?}").contains("schema_busy"));
    let unchanged = state.load(installation).expect("unchanged cache");
    assert_eq!(unchanged.observed_installed_version, Some(observed.clone()));
    assert!(unchanged.restart_needed);

    drop(old_schema);
    drop(old_owner);
    let mut admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(3),
        Duration::ZERO,
    )
    .expect("exclusive quiescence permits a truthful retry");
    admission
        .finish_after_store_ready()
        .expect("retry adopts after schema quiescence");
    drop(admission);
    let reconciled = state.load(installation).expect("reconciled cache");
    assert_eq!(reconciled.observed_installed_version, Some(current));
    assert!(!reconciled.restart_needed);
}
