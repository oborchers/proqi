use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    time::Duration,
};

use crate::{
    adapters::{
        memory::FakeIdGenerator, runtime::FileRuntimeCoordinator, update::FileUpdateStateStore,
    },
    domain::{
        Installation, InstallationIdentity, InstallationKind, ReleaseHighlightAnnouncement,
        StableVersion, Timestamp,
    },
    ports::{
        environment::IdGenerator as _,
        runtime::{Lease, RuntimeCoordinator as _, UpdateReplacementContext},
        update::{
            ExternalUpgradeAdoptionAuthority, ExternalUpgradeAuthorityError,
            UPDATE_CONTROL_PROTOCOL_VERSION, UpdateLockKind, UpdateStateStore as _,
        },
    },
};

use super::{StartupAdmission, admit_with_authority};

mod authority;
mod deferred;
mod locks;
mod replacements;

#[expect(
    clippy::too_many_arguments,
    reason = "startup tests keep exact installation, identity, and time inputs explicit"
)]
fn admit_with_timeout(
    cache_dir: &Path,
    coordinator: &FileRuntimeCoordinator,
    installation: InstallationIdentity,
    exact_resume: Option<crate::domain::SessionId>,
    current: &StableVersion,
    replacement: Option<&UpdateReplacementContext>,
    ids: &mut impl crate::ports::environment::IdGenerator,
    now: Timestamp,
    wait: Duration,
) -> Result<StartupAdmission, crate::cli::output::CliError> {
    let mut authority = SchemaOnlyAdoptionAuthority { coordinator };
    let installation = Installation {
        identity: installation,
        kind: InstallationKind::HomebrewFormula,
        executable: "/private/tmp/proqi".into(),
        restart_executable: Some("/private/tmp/proqi".into()),
    };
    admit_with_authority(
        cache_dir,
        coordinator,
        &installation,
        exact_resume,
        current,
        replacement,
        ids,
        now,
        wait,
        &mut authority,
    )
}

struct SchemaOnlyAdoptionAuthority<'a> {
    coordinator: &'a FileRuntimeCoordinator,
}

impl ExternalUpgradeAdoptionAuthority for SchemaOnlyAdoptionAuthority<'_> {
    fn revalidate_installation(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        Ok(())
    }

    fn acquire(&mut self) -> Result<Box<dyn Lease>, ExternalUpgradeAuthorityError> {
        self.coordinator
            .acquire_schema_exclusive()
            .map(|lease| Box::new(lease) as Box<dyn Lease>)
            .map_err(Into::into)
    }
}

fn coordinator(
    root: &std::path::Path,
    ids: &mut FakeIdGenerator,
    version: &str,
    installation: InstallationIdentity,
    protocol: u32,
) -> FileRuntimeCoordinator {
    FileRuntimeCoordinator::new(
        root.join("runtime"),
        ids.instance_id(),
        root.to_path_buf(),
        Timestamp::from_millis(1),
        version,
    )
    .expect("runtime coordinator")
    .with_update_context(installation, protocol, None)
}

#[test]
fn historical_owner_blocks_then_clean_shutdown_reconciles_without_refresh() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([92; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("stale restart state");
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let old = coordinator(temporary.path(), &mut ids, "0.9.0", installation, 1);
    let session_id = ids.session_id();
    let old_lease = old.acquire_session(session_id).expect("old live owner");
    let old_instance = old_lease.info().instance_id;
    let current_runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let Err(blocked) = admit_with_timeout(
        &cache,
        &current_runtime,
        installation,
        Some(session_id),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(2),
        Duration::ZERO,
    ) else {
        panic!("historical owner must block");
    };
    let diagnostic = format!("{blocked:?}");
    assert!(diagnostic.contains("external_upgrade_blocked"));
    assert!(diagnostic.contains(&session_id.to_string()));
    assert!(diagnostic.contains(&old_instance.to_string()));
    assert!(diagnostic.contains("0.9.0"));
    assert!(!diagnostic.contains("start 0.9.0"));

    drop(old_lease);
    let mut admission = admit_with_timeout(
        &cache,
        &current_runtime,
        installation,
        Some(session_id),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(3),
        Duration::ZERO,
    )
    .expect("verified newer installation should converge after shutdown");
    let deferred = state.load(installation).expect("deferred state");
    assert_eq!(deferred.observed_installed_version, Some(observed));
    assert!(deferred.restart_needed);
    admission
        .finish_after_store_ready()
        .expect("store-ready cache reconciliation");
    drop(admission);
    let reconciled = state.load(installation).expect("reconciled state");
    assert_eq!(reconciled.observed_installed_version, Some(current));
    assert!(!reconciled.restart_needed);
}

#[test]
fn equal_missing_and_malformed_observations_use_normal_admission() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([93; 32]);
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    let mut ids = FakeIdGenerator::new(1_800_100_000_000);
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let missing = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(1),
        Duration::ZERO,
    )
    .expect("missing cache is ordinary admission");
    drop(missing);
    state
        .record_restart_state(installation, current.clone(), true)
        .expect("equal cache");
    state
        .record_release_highlights(
            installation,
            ReleaseHighlightAnnouncement::pending(
                ids.session_id(),
                StableVersion::parse("0.9.0").expect("previous version"),
                current.clone(),
            )
            .expect("in-app restart announcement"),
        )
        .expect("pending in-app announcement");
    let equal = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(2),
        Duration::ZERO,
    )
    .expect("an in-app pending announcement keeps ordinary admission");
    drop(equal);

    let state_path = cache
        .join("updates")
        .join(installation.to_string())
        .join("state.json");
    fs::write(state_path, b"not json").expect("malformed cache");
    let malformed = admit_with_timeout(
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
    .expect("malformed cache is a cache miss");
    drop(malformed);
}

#[test]
fn obsolete_executable_and_active_convergence_fail_closed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([94; 32]);
    let old = StableVersion::parse("0.9.9").expect("old version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, current.clone(), false)
        .expect("current observation");
    let mut ids = FakeIdGenerator::new(1_800_200_000_000);
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.9.9",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let Err(obsolete) = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &old,
        None,
        &mut ids,
        Timestamp::from_millis(1),
        Duration::ZERO,
    ) else {
        panic!("obsolete binary must stay barred");
    };
    assert!(format!("{obsolete:?}").contains("obsolete_executable"));

    let convergence = state
        .try_lock(installation, UpdateLockKind::Convergence)
        .expect("convergence lock")
        .expect("convergence owner");
    let Err(active) = admit_with_timeout(
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
        panic!("active convergence must exclude startup");
    };
    assert!(format!("{active:?}").contains("update_convergence_active"));
    drop(convergence);
}

#[test]
fn stale_runtime_metadata_is_not_a_live_blocker() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([95; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed, true)
        .expect("stale observation");
    let mut ids = FakeIdGenerator::new(1_800_300_000_000);
    let old = coordinator(
        temporary.path(),
        &mut ids,
        "0.9.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION.saturating_sub(1),
    );
    let old_lease = old.acquire_session(ids.session_id()).expect("old owner");
    let stale_instance = old_lease.info().clone();
    let stale_path = temporary
        .path()
        .join("runtime/instances")
        .join(format!("{}.json", stale_instance.instance_id));
    drop(old_lease);
    fs::write(
        &stale_path,
        serde_json::to_vec(&stale_instance).expect("stale metadata bytes"),
    )
    .expect("restore stale metadata without its owner lock");
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );

    let mut admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(2),
        Duration::ZERO,
    )
    .expect("stale metadata cannot block adoption");
    admission
        .finish_after_store_ready()
        .expect("store-ready cache reconciliation");
    drop(admission);
    assert!(!stale_path.exists());
}

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
            let runtime = coordinator(
                &runtime_root,
                &mut ids,
                "0.10.0",
                installation,
                UPDATE_CONTROL_PROTOCOL_VERSION,
            );
            start.wait();
            let mut admission = admit_with_timeout(
                &cache,
                &runtime,
                installation,
                None,
                &current,
                None,
                &mut ids,
                Timestamp::from_millis(2),
                Duration::from_secs(2),
            )
            .expect("concurrent follower admission");
            admission
                .finish_after_store_ready()
                .expect("concurrent store-ready reconciliation");
            drop(admission);
        }));
    }
    for starter in starters {
        starter.join().expect("starter thread");
    }
    let reconciled = state.load(installation).expect("reconciled state");
    assert_eq!(reconciled.observed_installed_version, Some(current));
    assert!(!reconciled.restart_needed);
}

#[test]
fn unpublished_historical_startup_excludes_external_convergence() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([97; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("stale observation");
    let unpublished = state
        .try_startup_lock(installation)
        .expect("startup lock")
        .expect("unpublished historical startup");
    let mut ids = FakeIdGenerator::new(1_800_500_000_000);
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
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
        panic!("unpublished historical startup must exclude convergence");
    };
    assert!(format!("{blocked:?}").contains("update_convergence_active"));
    let unchanged = state.load(installation).expect("unchanged state");
    assert_eq!(unchanged.observed_installed_version, Some(observed));
    assert!(unchanged.restart_needed);

    drop(unpublished);
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
    .expect("retry after historical startup exits");
    admission
        .finish_after_store_ready()
        .expect("store-ready cache reconciliation");
    drop(admission);
}
