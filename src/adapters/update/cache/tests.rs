use std::{
    fs,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    application::{UpdateCheckMode, UpdateRefresh, UpdateService},
    domain::{
        Installation, InstallationIdentity, InstallationKind, ReleaseHighlightAnnouncement,
        StableVersion, Timestamp, UpdateCacheState,
    },
    ports::{
        environment::{Clock, IdGenerator as _},
        update::{
            InstallDetector, ReleaseObservation, ReleaseSource, UpdateError, UpdateLockKind,
            UpdateStateStore as _,
        },
    },
};

use super::FileUpdateStateStore;

fn identity() -> InstallationIdentity {
    InstallationIdentity::from_digest([19; 32])
}

fn announcement() -> ReleaseHighlightAnnouncement {
    let mut ids = crate::adapters::memory::FakeIdGenerator::new(1_800_000_000_000);
    ReleaseHighlightAnnouncement::pending(
        ids.session_id(),
        StableVersion::parse("0.3.0").expect("previous"),
        StableVersion::parse("0.4.0").expect("target"),
    )
    .expect("announcement")
}

struct Detector;

struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

impl InstallDetector for Detector {
    fn detect(&self) -> Result<Installation, UpdateError> {
        Ok(Installation {
            identity: identity(),
            kind: InstallationKind::StandaloneArchive,
            executable: "/verified/proqi".into(),
            restart_executable: None,
        })
    }
}

struct ContendedSource {
    calls: Arc<AtomicUsize>,
    attempted: Arc<Barrier>,
}

impl ReleaseSource for ContendedSource {
    fn latest_stable(
        &mut self,
        _: InstallationKind,
        _: Option<&str>,
    ) -> Result<ReleaseObservation, UpdateError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.attempted.wait();
        Ok(ReleaseObservation::Latest {
            version: StableVersion::parse("0.2.0").expect("latest"),
            etag: Some("safe-etag".to_owned()),
        })
    }
}

#[test]
fn corrupt_and_oversized_state_are_safe_cache_misses() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let directory = temporary
        .path()
        .join("updates")
        .join(identity().to_string());
    fs::create_dir_all(&directory).expect("installation directory");
    let state = directory.join("state.json");
    fs::write(&state, b"not json").expect("corrupt state");
    assert_eq!(store.load(identity()), Ok(UpdateCacheState::default()));
    fs::write(&state, vec![b'x'; 16 * 1024 + 1]).expect("oversized state");
    assert_eq!(store.load(identity()), Ok(UpdateCacheState::default()));
    let mut invalid = serde_json::to_value(UpdateCacheState {
        release_highlights: Some(announcement()),
        ..UpdateCacheState::default()
    })
    .expect("serialize state");
    invalid["release_highlights"]["previous_version"] = serde_json::json!("0.4.0");
    fs::write(
        &state,
        serde_json::to_vec(&invalid).expect("serialize invalid state"),
    )
    .expect("invalid announcement state");
    assert_eq!(store.load(identity()), Ok(UpdateCacheState::default()));
}

#[test]
fn refresh_prompt_and_installer_elections_are_independent() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    for kind in [
        UpdateLockKind::Refresh,
        UpdateLockKind::Prompt,
        UpdateLockKind::Installer,
    ] {
        let lease = store.try_lock(identity(), kind).expect("first lock");
        assert!(lease.is_some());
        assert!(
            store
                .try_lock(identity(), kind)
                .expect("contended lock")
                .is_none()
        );
        drop(lease);
        assert!(
            store
                .try_lock(identity(), kind)
                .expect("released lock")
                .is_some()
        );
    }
}

#[test]
fn refresh_generations_coalesce_stale_startups_but_not_explicit_checks() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let first = store
        .begin_refresh(identity(), Some(0))
        .expect("first startup")
        .expect("generation owner");
    assert_eq!(first.refresh_generation, 1);
    assert_eq!(
        store
            .begin_refresh(identity(), Some(0))
            .expect("stale startup"),
        None
    );
    let explicit = store
        .begin_refresh(identity(), None)
        .expect("explicit refresh")
        .expect("explicit owner");
    assert_eq!(explicit.refresh_generation, 2);
}

#[test]
fn fifteen_concurrent_startups_produce_one_refresh() {
    let temporary = tempfile::tempdir().expect("cache root");
    let state = Arc::new(FileUpdateStateStore::new(temporary.path()).expect("state"));
    let start = Arc::new(Barrier::new(15));
    let attempted = Arc::new(Barrier::new(15));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    for _ in 0..15 {
        let state = Arc::clone(&state);
        let start = Arc::clone(&start);
        let attempted = Arc::clone(&attempted);
        let calls = Arc::clone(&calls);
        threads.push(std::thread::spawn(move || {
            let clock = FixedClock(Timestamp::from_millis(1_800_000_000_000));
            let mut source = ContendedSource { calls, attempted };
            start.wait();
            let result = UpdateService::new(state.as_ref(), &mut source, &Detector, &clock)
                .check(
                    StableVersion::parse("0.1.0").expect("installed"),
                    UpdateCheckMode::Startup {
                        enabled: true,
                        release_build: true,
                        interactive: true,
                    },
                )
                .expect("check");
            if result.refresh == UpdateRefresh::InProgress {
                source.attempted.wait();
            }
            result.refresh
        }));
    }
    let outcomes = threads
        .into_iter()
        .map(|thread| thread.join().expect("contender"))
        .collect::<Vec<_>>();
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == UpdateRefresh::Refreshed)
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == UpdateRefresh::InProgress)
            .count(),
        14
    );
}

#[test]
fn fifteen_simultaneous_installer_contenders_elect_exactly_one_owner() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let start = Arc::new(Barrier::new(15));
    let attempted = Arc::new(Barrier::new(15));
    let winners = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    for _ in 0..15 {
        let store = store.clone();
        let start = Arc::clone(&start);
        let attempted = Arc::clone(&attempted);
        let winners = Arc::clone(&winners);
        threads.push(std::thread::spawn(move || {
            start.wait();
            let lease = store
                .try_lock(identity(), UpdateLockKind::Installer)
                .expect("installer election");
            if lease.is_some() {
                winners.fetch_add(1, Ordering::AcqRel);
            }
            attempted.wait();
            drop(lease);
        }));
    }
    for thread in threads {
        thread.join().expect("contender");
    }
    assert_eq!(winners.load(Ordering::Acquire), 1);
}

#[test]
fn successful_refresh_merges_global_dismissal_and_skip_state() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let first = StableVersion::parse("0.2.0").expect("first");
    let installed = StableVersion::parse("0.1.0").expect("installed");
    store
        .record_success(
            identity(),
            ReleaseObservation::Latest {
                version: first.clone(),
                etag: Some("\"first\"".to_owned()),
            },
            installed.clone(),
            Timestamp::from_millis(1),
        )
        .expect("first refresh");
    store.dismiss(identity(), first.clone()).expect("dismiss");
    store.skip(identity(), first).expect("skip");
    let second = StableVersion::parse("0.3.0").expect("second");
    let state = store
        .record_success(
            identity(),
            ReleaseObservation::Latest {
                version: second.clone(),
                etag: Some("\"second\"".to_owned()),
            },
            installed.clone(),
            Timestamp::from_millis(2),
        )
        .expect("second refresh");
    assert_eq!(state.latest_stable, Some(second));
    assert_eq!(state.dismissed_version, None);
    assert_eq!(state.skipped_version, None);
    assert_eq!(state.observed_installed_version, Some(installed));
}

#[test]
fn exact_release_highlight_acknowledgement_is_atomic_and_idempotent() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let expected = announcement();
    store
        .record_release_highlights(identity(), expected.clone())
        .expect("record announcement");
    assert!(
        store
            .acknowledge_release_highlights(identity(), &expected)
            .expect("acknowledge")
    );
    assert!(
        !store
            .acknowledge_release_highlights(identity(), &expected)
            .expect("idempotent acknowledgement")
    );
    let stored = store.load(identity()).expect("stored state");
    assert!(
        stored
            .release_highlights
            .is_some_and(|announcement| announcement.acknowledged())
    );
}

#[test]
fn mismatched_release_highlight_acknowledgement_changes_nothing() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let expected = announcement();
    store
        .record_release_highlights(identity(), expected.clone())
        .expect("record announcement");
    let mut ids = crate::adapters::memory::FakeIdGenerator::new(1_800_000_000_100);
    let mismatched = ReleaseHighlightAnnouncement::pending(
        ids.session_id(),
        expected.previous_version().clone(),
        expected.target_version().clone(),
    )
    .expect("mismatch");
    assert!(
        !store
            .acknowledge_release_highlights(identity(), &mismatched)
            .expect("mismatched acknowledgement")
    );
    assert_eq!(
        store.load(identity()).expect("stored").release_highlights,
        Some(expected)
    );
}

#[cfg(unix)]
#[test]
fn cache_directories_and_files_are_user_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = tempfile::tempdir().expect("cache root");
    let root = temporary.path().join("private-cache");
    let store = FileUpdateStateStore::new(&root).expect("store");
    store
        .record_success(
            identity(),
            ReleaseObservation::Latest {
                version: StableVersion::parse("0.2.0").expect("version"),
                etag: None,
            },
            StableVersion::parse("0.1.0").expect("installed"),
            Timestamp::from_millis(1),
        )
        .expect("write state");
    let directory = root.join("updates").join(identity().to_string());
    let state = directory.join("state.json");
    assert_eq!(
        fs::metadata(directory)
            .expect("directory")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(state).expect("state").permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn symlinked_state_is_refused_without_touching_target() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let directory = temporary
        .path()
        .join("updates")
        .join(identity().to_string());
    fs::create_dir_all(&directory).expect("installation directory");
    let target = temporary.path().join("target");
    fs::write(&target, b"preserve").expect("target");
    symlink(&target, directory.join("state.json")).expect("state symlink");
    assert!(store.load(identity()).is_err());
    assert_eq!(fs::read(target).expect("target content"), b"preserve");
}
