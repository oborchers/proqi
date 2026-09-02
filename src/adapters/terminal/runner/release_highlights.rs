//! Quiet startup selection of packaged highlights after board restoration.

use std::path::Path;

use crate::{
    adapters::update::{FileUpdateStateStore, packaged_release_highlights},
    application::ReleaseHighlightSelection,
    domain::{Installation, SessionId, StableVersion},
    ports::update::{RestartCompletion, UpdateStateStore as _},
};

pub(super) fn load(
    cache_directory: &Path,
    installation: Option<&Installation>,
    session_id: SessionId,
) -> ReleaseHighlightSelection {
    let Ok(manifest) = packaged_release_highlights() else {
        return ReleaseHighlightSelection::default();
    };
    let Ok(installed) = StableVersion::parse(env!("CARGO_PKG_VERSION")) else {
        return ReleaseHighlightSelection::default();
    };
    let cache = installation
        .and_then(|installation| {
            FileUpdateStateStore::new(cache_directory)
                .and_then(|state| state.load(installation.identity))
                .ok()
        })
        .unwrap_or_default();
    ReleaseHighlightSelection::select(&manifest, &cache, session_id, &installed)
}

pub(super) fn mark_restart_ready(
    cache_directory: &Path,
    installation: Option<&Installation>,
    selection: &ReleaseHighlightSelection,
    control_ready: bool,
) -> bool {
    let (Some(installation), Some(presentation)) = (installation, selection.automatic.as_ref())
    else {
        return false;
    };
    if !control_ready {
        record_finalization_failure(
            crate::adapters::diagnostics::UpdateFinalizationFailure::ControlUnavailable,
        );
        return false;
    }
    let Ok(state) = FileUpdateStateStore::new(cache_directory) else {
        record_finalization_failure(
            crate::adapters::diagnostics::UpdateFinalizationFailure::StateUnavailable,
        );
        return false;
    };
    match state.complete_restart(installation.identity, &presentation.announcement) {
        Ok(RestartCompletion::Completed) => {
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::UpdateConverged,
            );
            true
        }
        Ok(RestartCompletion::AlreadyComplete) => true,
        Ok(RestartCompletion::Mismatch) => {
            record_finalization_failure(
                crate::adapters::diagnostics::UpdateFinalizationFailure::StateMismatch,
            );
            false
        }
        Err(_) => {
            record_finalization_failure(
                crate::adapters::diagnostics::UpdateFinalizationFailure::StateUnavailable,
            );
            false
        }
    }
}

fn record_finalization_failure(code: crate::adapters::diagnostics::UpdateFinalizationFailure) {
    crate::adapters::diagnostics::record(
        crate::adapters::diagnostics::SafeEvent::UpdateFinalizationFailed { code },
    );
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::update::FileUpdateStateStore,
        application::{ReleaseHighlightPresentation, ReleaseHighlightSelection},
        domain::{
            Installation, InstallationIdentity, InstallationKind, ReleaseHighlightAnnouncement,
            StableVersion,
        },
        ports::{
            environment::IdGenerator as _,
            update::{RestartCompletion, UpdateStateStore as _},
        },
    };

    use super::mark_restart_ready;

    #[test]
    fn exact_initiating_board_and_control_readiness_clear_restart_needed() {
        let temporary = tempfile::tempdir().expect("cache directory");
        let state = FileUpdateStateStore::new(temporary.path()).expect("update state");
        let identity = InstallationIdentity::from_digest([9; 32]);
        let mut ids = crate::adapters::memory::FakeIdGenerator::new(1_800_000_000_000);
        let target = StableVersion::parse("1.2.0").expect("target");
        let announcement = ReleaseHighlightAnnouncement::pending(
            ids.session_id(),
            StableVersion::parse("1.1.0").expect("previous"),
            target.clone(),
        )
        .expect("announcement");
        let installation = Installation {
            identity,
            kind: InstallationKind::HomebrewFormula,
            executable: std::path::PathBuf::from("/synthetic/proqi"),
            restart_executable: None,
        };
        let selection = ReleaseHighlightSelection {
            installed: None,
            automatic: Some(ReleaseHighlightPresentation {
                announcement: announcement.clone(),
                groups: Vec::new(),
            }),
        };
        state
            .record_release_highlights(identity, announcement.clone())
            .expect("pending highlights");

        for (control_ready, expected) in [(false, true), (true, false)] {
            state
                .record_restart_state(identity, target.clone(), true)
                .expect("pending restart");
            let visible = mark_restart_ready(
                temporary.path(),
                Some(&installation),
                &selection,
                control_ready,
            );
            assert_eq!(visible, control_ready);
            assert_eq!(
                state.load(identity).expect("cache").restart_needed,
                expected
            );
        }
        assert_eq!(
            state.complete_restart(identity, &announcement),
            Ok(RestartCompletion::AlreadyComplete),
            "completed transition is emitted at most once"
        );
        let newer = StableVersion::parse("1.3.0").expect("newer target");
        state
            .record_restart_state(identity, newer.clone(), true)
            .expect("newer restart");
        assert_eq!(
            state.complete_restart(identity, &announcement),
            Ok(RestartCompletion::Mismatch)
        );
        assert!(!mark_restart_ready(
            temporary.path(),
            Some(&installation),
            &selection,
            true,
        ));
        let invalid_cache = tempfile::NamedTempFile::new().expect("cache file");
        assert!(!mark_restart_ready(
            invalid_cache.path(),
            Some(&installation),
            &selection,
            true,
        ));
        let cache = state.load(identity).expect("newer cache");
        assert!(cache.restart_needed);
        assert_eq!(cache.observed_installed_version, Some(newer));
    }
}
