//! Canonical validation and release merge rules for durable update cache state.

use crate::{
    domain::{ReleaseHighlightAnnouncement, StableVersion, UpdateCacheState},
    ports::update::RestartCompletion,
};

use super::super::etag::valid as valid_etag;

pub(super) fn valid_loaded_state(state: &UpdateCacheState) -> bool {
    if state.etag.as_deref().is_some_and(|etag| !valid_etag(etag)) {
        return false;
    }
    !state.external_restart.as_ref().is_some_and(|pending| {
        !state.restart_needed
            || state.observed_installed_version.as_ref() != Some(pending.target_version())
    })
}

pub(super) fn merge_latest(
    state: &mut UpdateCacheState,
    version: StableVersion,
    etag: Option<String>,
) {
    if state.skipped_version.as_ref() != Some(&version) {
        state.skipped_version = None;
    }
    state.latest_stable = Some(version);
    state.etag = etag.filter(|value| valid_etag(value));
}

pub(super) fn classify_restart_completion(
    state: &mut UpdateCacheState,
    announcement: &ReleaseHighlightAnnouncement,
) -> RestartCompletion {
    if state.external_restart.is_some() {
        return RestartCompletion::Mismatch;
    }
    let exact_target =
        state.observed_installed_version.as_ref() == Some(announcement.target_version());
    let exact_announcement = state
        .release_highlights
        .as_ref()
        .is_some_and(|current| !current.acknowledged() && current.same_upgrade(announcement));
    if !exact_target || !exact_announcement {
        return RestartCompletion::Mismatch;
    }
    if !state.restart_needed {
        return RestartCompletion::AlreadyComplete;
    }
    state.restart_needed = false;
    RestartCompletion::Completed
}
