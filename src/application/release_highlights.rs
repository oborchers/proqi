//! Truthful release-highlight selection from packaged content and private update state.

use crate::domain::{
    ReleaseHighlightAnnouncement, ReleaseHighlightGroup, ReleaseHighlightsManifest, SessionId,
    StableVersion, UpdateCacheState,
};

/// Installed content and an optional exact-session automatic announcement.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReleaseHighlightSelection {
    /// Exact installed release available from the command palette.
    pub installed: Option<ReleaseHighlightGroup>,
    /// Verified pending announcement for this resumed session only.
    pub automatic: Option<ReleaseHighlightPresentation>,
}

/// Packaged skipped-version groups paired with their content-free durable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseHighlightPresentation {
    /// Exact record that must be acknowledged after explicit dismissal.
    pub announcement: ReleaseHighlightAnnouncement,
    /// Releases newer than the prior version through the exact target.
    pub groups: Vec<ReleaseHighlightGroup>,
}

impl ReleaseHighlightSelection {
    /// Select manual and automatic content without inferring absent or mismatched state.
    #[must_use]
    pub fn select(
        manifest: &ReleaseHighlightsManifest,
        cache: &UpdateCacheState,
        session_id: SessionId,
        installed: &StableVersion,
    ) -> Self {
        let installed_group = manifest.installed(installed);
        let automatic = cache.release_highlights.as_ref().and_then(|announcement| {
            if !announcement.is_pending_for(session_id, installed) {
                return None;
            }
            manifest
                .between(
                    announcement.previous_version(),
                    announcement.target_version(),
                )
                .map(|groups| ReleaseHighlightPresentation {
                    announcement: announcement.clone(),
                    groups,
                })
        });
        Self {
            installed: installed_group,
            automatic,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{ReleaseHighlightAnnouncement, ReleaseHighlightsManifest, StableVersion},
        ports::environment::IdGenerator as _,
    };

    use super::ReleaseHighlightSelection;

    fn manifest() -> ReleaseHighlightsManifest {
        ReleaseHighlightsManifest::parse_json(
            r#"{"schema_version":1,"releases":[{"version":"1.0.0","highlights":["One","Two","Three"]},{"version":"1.1.0","highlights":["Four","Five","Six"]},{"version":"1.2.0","highlights":["Seven","Eight","Nine"]}]}"#,
        )
        .expect("manifest")
    }

    #[test]
    fn exact_session_and_target_select_skipped_versions() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let session = ids.session_id();
        let announcement = ReleaseHighlightAnnouncement::pending(
            session,
            StableVersion::parse("1.0.0").expect("previous"),
            StableVersion::parse("1.2.0").expect("target"),
        )
        .expect("announcement");
        let cache = crate::domain::UpdateCacheState {
            release_highlights: Some(announcement),
            ..crate::domain::UpdateCacheState::default()
        };
        let selected = ReleaseHighlightSelection::select(
            &manifest(),
            &cache,
            session,
            &StableVersion::parse("1.2.0").expect("installed"),
        );
        assert_eq!(selected.automatic.expect("automatic").groups.len(), 2);
    }

    #[test]
    fn peer_session_and_target_mismatch_remain_quiet() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let initiating = ids.session_id();
        let announcement = ReleaseHighlightAnnouncement::pending(
            initiating,
            StableVersion::parse("1.0.0").expect("previous"),
            StableVersion::parse("1.2.0").expect("target"),
        )
        .expect("announcement");
        let cache = crate::domain::UpdateCacheState {
            release_highlights: Some(announcement),
            ..crate::domain::UpdateCacheState::default()
        };
        assert!(
            ReleaseHighlightSelection::select(
                &manifest(),
                &cache,
                ids.session_id(),
                &StableVersion::parse("1.2.0").expect("installed"),
            )
            .automatic
            .is_none()
        );
        assert!(
            ReleaseHighlightSelection::select(
                &manifest(),
                &cache,
                initiating,
                &StableVersion::parse("1.1.0").expect("installed"),
            )
            .automatic
            .is_none()
        );
        let mut acknowledged = cache.clone();
        acknowledged
            .release_highlights
            .as_mut()
            .expect("announcement")
            .acknowledge();
        assert!(
            ReleaseHighlightSelection::select(
                &manifest(),
                &acknowledged,
                initiating,
                &StableVersion::parse("1.2.0").expect("installed"),
            )
            .automatic
            .is_none()
        );
    }
}
