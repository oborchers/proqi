use std::{cell::RefCell, collections::BTreeSet, path::PathBuf};

use crate::{
    application::test_support::TestClock,
    domain::{
        Installation, InstallationIdentity, InstallationKind, ReleaseHighlightAnnouncement,
        StableVersion, Timestamp, UpdateCacheState,
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

    fn begin_refresh(
        &self,
        _: InstallationIdentity,
        observed_generation: Option<u64>,
    ) -> Result<Option<UpdateCacheState>, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        if observed_generation.is_some_and(|generation| generation != cache.refresh_generation) {
            return Ok(None);
        }
        cache.refresh_generation = cache
            .refresh_generation
            .checked_add(1)
            .ok_or_else(|| UpdateError::State("generation exhausted".to_owned()))?;
        Ok(Some(cache.clone()))
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

    fn record_release_highlights(
        &self,
        _: InstallationIdentity,
        announcement: ReleaseHighlightAnnouncement,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.cache.borrow_mut().release_highlights = Some(announcement);
        Ok(self.cache.borrow().clone())
    }

    fn acknowledge_release_highlights(
        &self,
        _: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        let Some(current) = cache.release_highlights.as_mut() else {
            return Ok(false);
        };
        if current.acknowledged() || !current.same_upgrade(announcement) {
            return Ok(false);
        }
        current.acknowledge();
        Ok(true)
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

impl State {
    fn release_refresh_lock(&self) {
        self.locks
            .borrow_mut()
            .remove(&(UpdateLockKind::Refresh as u8));
    }
}

impl ReleaseSource for Source {
    fn latest_stable(
        &mut self,
        _: InstallationKind,
        _: Option<&str>,
    ) -> Result<ReleaseObservation, UpdateError> {
        self.calls += 1;
        self.result.clone()
    }
}

#[test]
fn startup_exclusions_never_contact_the_source() {
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
                UpdateCheckMode::Startup {
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
fn every_successive_startup_refreshes_even_when_the_cache_is_recent() {
    let state = State::default();
    state.cache.replace(UpdateCacheState {
        latest_stable: Some(StableVersion::parse("0.1.0").expect("latest")),
        last_checked_at: Some(Timestamp::from_millis(1_799_999_999_999)),
        ..UpdateCacheState::default()
    });
    let detector = Detector(InstallationKind::StandaloneArchive);
    let clock = TestClock(Timestamp::from_millis(1_800_000_000_000));
    let mut source = Source {
        calls: 0,
        result: Ok(ReleaseObservation::NotModified),
    };

    for expected_generation in 1..=2 {
        let result = UpdateService::new(&state, &mut source, &detector, &clock)
            .check(
                StableVersion::parse("0.1.0").expect("installed"),
                UpdateCheckMode::Startup {
                    enabled: true,
                    release_build: true,
                    interactive: true,
                },
            )
            .expect("startup check");
        assert_eq!(result.availability, UpdateAvailability::Current);
        assert_eq!(result.refresh, UpdateRefresh::Refreshed);
        assert_eq!(state.cache.borrow().refresh_generation, expected_generation);
        state.release_refresh_lock();
    }
    assert_eq!(source.calls, 2);
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
        result: Ok(ReleaseObservation::Latest {
            version: StableVersion::parse("0.3.0").expect("latest"),
            etag: None,
        }),
    };

    let result = UpdateService::new(&state, &mut source, &detector, &clock)
        .check(
            StableVersion::parse("0.1.0").expect("installed"),
            UpdateCheckMode::Startup {
                enabled: true,
                release_build: true,
                interactive: true,
            },
        )
        .expect("later release");

    assert_eq!(result.availability, UpdateAvailability::Available);
    assert_eq!(source.calls, 1);
}

#[test]
fn a_successful_startup_refresh_clears_not_now_but_preserves_an_exact_skip() {
    let state = State::default();
    state.cache.replace(UpdateCacheState {
        latest_stable: Some(StableVersion::parse("0.2.0").expect("cached")),
        dismissed_version: Some(StableVersion::parse("0.2.0").expect("dismissed")),
        skipped_version: Some(StableVersion::parse("0.2.0").expect("skipped")),
        ..UpdateCacheState::default()
    });
    let detector = Detector(InstallationKind::StandaloneArchive);
    let clock = TestClock(Timestamp::from_millis(1_800_000_000_001));
    let mut source = Source {
        calls: 0,
        result: Ok(ReleaseObservation::NotModified),
    };

    let result = UpdateService::new(&state, &mut source, &detector, &clock)
        .check(
            StableVersion::parse("0.1.0").expect("installed"),
            UpdateCheckMode::Startup {
                enabled: true,
                release_build: true,
                interactive: true,
            },
        )
        .expect("startup refresh");

    assert_eq!(result.refresh, UpdateRefresh::Refreshed);
    assert_eq!(result.availability, UpdateAvailability::Suppressed);
    assert_eq!(state.cache.borrow().dismissed_version, None);
    assert_eq!(
        state.cache.borrow().skipped_version,
        Some(StableVersion::parse("0.2.0").expect("skipped"))
    );
    assert_eq!(source.calls, 1);
}
