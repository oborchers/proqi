use std::{cell::RefCell, collections::BTreeSet, path::PathBuf};

use crate::{
    application::test_support::TestClock,
    domain::{
        Installation, InstallationIdentity, InstallationKind, StableVersion, Timestamp,
        UpdateCacheState,
    },
    ports::update::{
        InstallDetector, ReleaseObservation, ReleaseSource, UpdateError, UpdateLease,
        UpdateLockKind, UpdateStateStore,
    },
};

use super::{UpdateAvailability, UpdateCheckMode, UpdateRefresh, UpdateService};

struct Lease;
impl UpdateLease for Lease {}

#[derive(Default)]
struct State {
    cache: RefCell<UpdateCacheState>,
    locks: RefCell<BTreeSet<u8>>,
}

impl UpdateStateStore for State {
    fn load(&self, _: InstallationIdentity) -> Result<UpdateCacheState, UpdateError> {
        Ok(self.cache.borrow().clone())
    }

    fn try_lock(
        &self,
        _: InstallationIdentity,
        kind: UpdateLockKind,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        let key = kind as u8;
        if self.locks.borrow_mut().insert(key) {
            Ok(Some(Box::new(Lease)))
        } else {
            Ok(None)
        }
    }

    fn record_success(
        &self,
        _: InstallationIdentity,
        observed: ReleaseObservation,
        installed: StableVersion,
        checked_at: Timestamp,
    ) -> Result<UpdateCacheState, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        if let ReleaseObservation::Latest { version, etag } = observed {
            if cache.skipped_version.as_ref() != Some(&version) {
                cache.skipped_version = None;
            }
            cache.latest_stable = Some(version);
            cache.etag = etag;
        }
        cache.dismissed_version = None;
        cache.observed_installed_version = Some(installed);
        cache.last_checked_at = Some(checked_at);
        Ok(cache.clone())
    }

    fn dismiss(
        &self,
        _: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.cache.borrow_mut().dismissed_version = Some(version);
        Ok(self.cache.borrow().clone())
    }

    fn skip(
        &self,
        _: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.cache.borrow_mut().skipped_version = Some(version);
        Ok(self.cache.borrow().clone())
    }

    fn record_restart_state(
        &self,
        _: InstallationIdentity,
        installed: StableVersion,
        restart_needed: bool,
    ) -> Result<UpdateCacheState, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        cache.observed_installed_version = Some(installed);
        cache.restart_needed = restart_needed;
        Ok(cache.clone())
    }
}

struct Detector(InstallationKind);

impl InstallDetector for Detector {
    fn detect(&self) -> Result<Installation, UpdateError> {
        Ok(Installation {
            identity: InstallationIdentity::from_digest([7; 32]),
            kind: self.0,
            executable: PathBuf::from("/verified/proqi"),
            restart_executable: None,
        })
    }
}

struct Source {
    calls: usize,
    result: Result<ReleaseObservation, UpdateError>,
}

impl ReleaseSource for Source {
    fn latest_stable(&mut self, _: Option<&str>) -> Result<ReleaseObservation, UpdateError> {
        self.calls += 1;
        self.result.clone()
    }
}

#[test]
fn fifteen_implicit_contenders_produce_one_refresh() {
    let state = State::default();
    let detector = Detector(InstallationKind::StandaloneArchive);
    let clock = TestClock(Timestamp::from_millis(1_800_000_000_000));
    let mut source = Source {
        calls: 0,
        result: Ok(ReleaseObservation::Latest {
            version: StableVersion::parse("0.2.0").expect("latest"),
            etag: Some("safe-etag".to_owned()),
        }),
    };
    for _ in 0..15 {
        let result = UpdateService::new(&state, &mut source, &detector, &clock)
            .check(
                StableVersion::parse("0.1.0").expect("installed"),
                UpdateCheckMode::Implicit {
                    enabled: true,
                    release_build: true,
                    interactive: true,
                },
            )
            .expect("check");
        assert_eq!(result.availability, super::UpdateAvailability::Available);
    }
    assert_eq!(source.calls, 1);
}

#[test]
fn implicit_exclusions_never_contact_the_source() {
    for (kind, enabled, release_build, interactive) in [
        (InstallationKind::StandaloneArchive, false, true, true),
        (InstallationKind::StandaloneArchive, true, false, true),
        (InstallationKind::StandaloneArchive, true, true, false),
        (InstallationKind::SourceOrUnknown, true, true, true),
    ] {
        let state = State::default();
        let detector = Detector(kind);
        let clock = TestClock(Timestamp::from_millis(1));
        let mut source = Source {
            calls: 0,
            result: Err(UpdateError::Network),
        };
        let _result = UpdateService::new(&state, &mut source, &detector, &clock)
            .check(
                StableVersion::parse("0.1.0").expect("installed"),
                UpdateCheckMode::Implicit {
                    enabled,
                    release_build,
                    interactive,
                },
            )
            .expect("excluded check");
        assert_eq!(source.calls, 0);
    }
}

#[test]
fn explicit_check_reports_network_failure_without_cached_claims() {
    let state = State::default();
    let detector = Detector(InstallationKind::SourceOrUnknown);
    let clock = TestClock(Timestamp::from_millis(1));
    let mut source = Source {
        calls: 0,
        result: Err(UpdateError::Network),
    };
    let result = UpdateService::new(&state, &mut source, &detector, &clock).check(
        StableVersion::parse("0.1.0").expect("installed"),
        UpdateCheckMode::Explicit,
    );
    assert_eq!(result, Err(UpdateError::Network));
    assert_eq!(source.calls, 1);
}

#[test]
fn fresh_cache_is_nonblocking_and_compares_equal_or_older_versions_correctly() {
    for latest in ["0.1.0", "0.0.9"] {
        let state = State::default();
        state.cache.replace(UpdateCacheState {
            latest_stable: Some(StableVersion::parse(latest).expect("latest")),
            last_checked_at: Some(Timestamp::from_millis(1_799_999_999_999)),
            ..UpdateCacheState::default()
        });
        let detector = Detector(InstallationKind::StandaloneArchive);
        let clock = TestClock(Timestamp::from_millis(1_800_000_000_000));
        let mut source = Source {
            calls: 0,
            result: Err(UpdateError::Network),
        };

        let result = UpdateService::new(&state, &mut source, &detector, &clock)
            .check(
                StableVersion::parse("0.1.0").expect("installed"),
                UpdateCheckMode::Implicit {
                    enabled: true,
                    release_build: true,
                    interactive: true,
                },
            )
            .expect("cached check");

        assert_eq!(result.availability, UpdateAvailability::Current);
        assert_eq!(result.refresh, UpdateRefresh::Cached);
        assert_eq!(source.calls, 0);
    }
}

#[test]
fn exact_suppression_does_not_hide_a_later_release() {
    let state = State::default();
    state.cache.replace(UpdateCacheState {
        latest_stable: Some(StableVersion::parse("0.3.0").expect("latest")),
        dismissed_version: Some(StableVersion::parse("0.2.0").expect("dismissed")),
        skipped_version: Some(StableVersion::parse("0.2.0").expect("skipped")),
        last_checked_at: Some(Timestamp::from_millis(1)),
        ..UpdateCacheState::default()
    });
    let detector = Detector(InstallationKind::StandaloneArchive);
    let clock = TestClock(Timestamp::from_millis(2));
    let mut source = Source {
        calls: 0,
        result: Err(UpdateError::Network),
    };

    let result = UpdateService::new(&state, &mut source, &detector, &clock)
        .check(
            StableVersion::parse("0.1.0").expect("installed"),
            UpdateCheckMode::Implicit {
                enabled: true,
                release_build: true,
                interactive: true,
            },
        )
        .expect("later release");

    assert_eq!(result.availability, UpdateAvailability::Available);
    assert_eq!(source.calls, 0);
}

#[test]
fn exact_twenty_four_hour_boundary_is_stale() {
    let state = State::default();
    state.cache.replace(UpdateCacheState {
        latest_stable: Some(StableVersion::parse("0.1.0").expect("cached")),
        last_checked_at: Some(Timestamp::from_millis(1_800_000_000_000)),
        ..UpdateCacheState::default()
    });
    let detector = Detector(InstallationKind::StandaloneArchive);
    let clock = TestClock(Timestamp::from_millis(1_800_086_400_000));
    let mut source = Source {
        calls: 0,
        result: Ok(ReleaseObservation::Latest {
            version: StableVersion::parse("0.2.0").expect("latest"),
            etag: None,
        }),
    };

    let result = UpdateService::new(&state, &mut source, &detector, &clock)
        .check(
            StableVersion::parse("0.1.0").expect("installed"),
            UpdateCheckMode::Implicit {
                enabled: true,
                release_build: true,
                interactive: true,
            },
        )
        .expect("stale refresh");

    assert_eq!(result.refresh, UpdateRefresh::Refreshed);
    assert_eq!(source.calls, 1);
}
