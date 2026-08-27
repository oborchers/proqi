//! Installation-wide update decision orchestration.

use serde::Serialize;

use crate::{
    domain::{InstallationKind, StableVersion, UpdateCacheState},
    ports::{
        environment::Clock,
        update::{InstallDetector, ReleaseSource, UpdateError, UpdateLockKind, UpdateStateStore},
    },
};

/// Why an update check is being considered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateCheckMode {
    /// User explicitly invoked `proqi update check`.
    Explicit,
    /// Each eligible interactive startup schedules a background refresh.
    Startup {
        /// Global configuration switch.
        enabled: bool,
        /// Compiled without debug assertions as a release product.
        release_build: bool,
        /// Launch owns an interactive terminal.
        interactive: bool,
    },
}

/// Bounded machine and UI update-check outcome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UpdateCheckResult {
    /// Verified installation mechanism.
    pub installation: InstallationKind,
    /// Version of the running executable.
    pub installed_version: StableVersion,
    /// Latest stable version known from the cache or refresh.
    pub latest_version: Option<StableVersion>,
    /// Latest-version relationship including global prompt suppression.
    pub availability: UpdateAvailability,
    /// How release metadata was obtained for this result.
    pub refresh: UpdateRefresh,
    /// Canonical release page, present only when an update is available.
    pub release_url: Option<&'static str>,
}

/// Latest stable release relationship to the running process.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAvailability {
    /// No newer stable release is known.
    Current,
    /// A newer stable release should produce an actionable prompt.
    Available,
    /// A newer stable release was globally dismissed or skipped.
    Suppressed,
}

/// Installation-wide refresh disposition for one check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateRefresh {
    /// Existing private cache was used.
    Cached,
    /// This call refreshed release metadata.
    Refreshed,
    /// Another process owns the one permitted refresh.
    InProgress,
    /// Another startup already refreshed the generation observed by this process.
    Coalesced,
}

/// Coordinates update decisions through injected platform boundaries.
pub struct UpdateService<'a, S, R, D, C> {
    state: &'a S,
    source: &'a mut R,
    detector: &'a D,
    clock: &'a C,
}

impl<'a, S, R, D, C> UpdateService<'a, S, R, D, C>
where
    S: UpdateStateStore,
    R: ReleaseSource,
    D: InstallDetector,
    C: Clock,
{
    /// Construct the update application facade.
    #[must_use]
    pub const fn new(state: &'a S, source: &'a mut R, detector: &'a D, clock: &'a C) -> Self {
        Self {
            state,
            source,
            detector,
            clock,
        }
    }

    /// Read cached state and perform at most one elected refresh.
    ///
    /// # Errors
    ///
    /// Explicit checks return typed installation, cache, and network failures.
    /// Startup callers should treat failures as quiet diagnostics.
    pub fn check(
        &mut self,
        installed: StableVersion,
        mode: UpdateCheckMode,
    ) -> Result<UpdateCheckResult, UpdateError> {
        let installation = self.detector.detect()?;
        let state = self.state.load(installation.identity)?;
        if !should_refresh(installation.kind, mode) {
            return Ok(result(
                installation.kind,
                installed,
                state,
                UpdateRefresh::Cached,
            ));
        }
        let Some(refresh_lease) = self
            .state
            .try_lock(installation.identity, UpdateLockKind::Refresh)?
        else {
            return Ok(result(
                installation.kind,
                installed,
                state,
                UpdateRefresh::InProgress,
            ));
        };
        let observed_generation = match mode {
            UpdateCheckMode::Explicit => None,
            UpdateCheckMode::Startup { .. } => Some(state.refresh_generation),
        };
        let Some(refresh_state) = self
            .state
            .begin_refresh(installation.identity, observed_generation)?
        else {
            drop(refresh_lease);
            let current = self.state.load(installation.identity)?;
            return Ok(result(
                installation.kind,
                installed,
                current,
                UpdateRefresh::Coalesced,
            ));
        };
        let observation = self
            .source
            .latest_stable(installation.kind, refresh_state.etag.as_deref())?;
        let now = self.clock.now();
        let state = self.state.record_success(
            installation.identity,
            observation,
            installed.clone(),
            now,
        )?;
        drop(refresh_lease);
        Ok(result(
            installation.kind,
            installed,
            state,
            UpdateRefresh::Refreshed,
        ))
    }
}

fn should_refresh(installation: InstallationKind, mode: UpdateCheckMode) -> bool {
    match mode {
        UpdateCheckMode::Explicit => true,
        UpdateCheckMode::Startup {
            enabled,
            release_build,
            interactive,
        } => {
            enabled
                && release_build
                && interactive
                && installation != InstallationKind::SourceOrUnknown
        }
    }
}

fn result(
    installation: InstallationKind,
    installed_version: StableVersion,
    state: UpdateCacheState,
    refresh: UpdateRefresh,
) -> UpdateCheckResult {
    let update_available = state
        .latest_stable
        .as_ref()
        .is_some_and(|latest| latest > &installed_version);
    let suppressed = state.latest_stable.as_ref().is_some_and(|latest| {
        state.skipped_version.as_ref() == Some(latest)
            || state.dismissed_version.as_ref() == Some(latest)
    });
    let availability = if !update_available {
        UpdateAvailability::Current
    } else if suppressed {
        UpdateAvailability::Suppressed
    } else {
        UpdateAvailability::Available
    };
    UpdateCheckResult {
        installation,
        installed_version,
        latest_version: state.latest_stable,
        availability,
        refresh,
        release_url: update_available
            .then_some("https://github.com/oborchers/proqi/releases/latest"),
    }
}

#[cfg(test)]
mod tests;
