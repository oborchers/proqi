use std::fs;

use crate::{
    domain::{InstallationIdentity, StableVersion, Timestamp, UpdateCacheState},
    ports::update::{ReleaseObservation, UpdateLockKind, UpdateStateStore as _},
};

use super::FileUpdateStateStore;

fn identity() -> InstallationIdentity {
    InstallationIdentity::from_digest([19; 32])
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
