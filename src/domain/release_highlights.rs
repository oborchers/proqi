//! Versioned user-facing highlights embedded in each installed executable.

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use super::{SessionId, StableVersion};

/// Maximum checked-in manifest size accepted by the product and release tooling.
pub const RELEASE_HIGHLIGHTS_MAX_BYTES: usize = 64 * 1024;
/// Minimum reviewed highlights required for one represented release.
pub const RELEASE_HIGHLIGHTS_MIN_ITEMS: usize = 3;
/// Maximum reviewed highlights permitted for one represented release.
pub const RELEASE_HIGHLIGHTS_MAX_ITEMS: usize = 6;
/// Maximum Unicode scalar values accepted in one concise highlight.
pub const RELEASE_HIGHLIGHT_MAX_CHARS: usize = 240;

/// Content-free durable identity and acknowledgement of one in-app upgrade announcement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleaseHighlightAnnouncement {
    session_id: SessionId,
    previous_version: StableVersion,
    target_version: StableVersion,
    acknowledged: bool,
}

impl ReleaseHighlightAnnouncement {
    /// Construct one pending exact-version announcement for its initiating session.
    ///
    /// # Errors
    ///
    /// Rejects an update that does not advance to a newer stable version.
    pub fn pending(
        session_id: SessionId,
        previous_version: StableVersion,
        target_version: StableVersion,
    ) -> Result<Self, ReleaseHighlightAnnouncementError> {
        if previous_version >= target_version {
            return Err(ReleaseHighlightAnnouncementError::InvalidVersionRange);
        }
        Ok(Self {
            session_id,
            previous_version,
            target_version,
            acknowledged: false,
        })
    }

    /// Exact initiating session.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Version from which the in-app upgrade began.
    #[must_use]
    pub const fn previous_version(&self) -> &StableVersion {
        &self.previous_version
    }

    /// Exact version verified after installation.
    #[must_use]
    pub const fn target_version(&self) -> &StableVersion {
        &self.target_version
    }

    /// Whether explicit dismissal was durably acknowledged.
    #[must_use]
    pub const fn acknowledged(&self) -> bool {
        self.acknowledged
    }

    /// Match an unacknowledged announcement to one resumed session and executable.
    #[must_use]
    pub fn is_pending_for(&self, session_id: SessionId, installed: &StableVersion) -> bool {
        !self.acknowledged && self.session_id == session_id && &self.target_version == installed
    }

    /// Match one exact acknowledgement request without trusting a stale target alone.
    #[must_use]
    pub fn same_upgrade(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.previous_version == other.previous_version
            && self.target_version == other.target_version
    }

    /// Mark this exact record acknowledged after a matching explicit dismissal.
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

impl<'de> Deserialize<'de> for ReleaseHighlightAnnouncement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAnnouncement::deserialize(deserializer)?;
        let mut announcement =
            Self::pending(raw.session_id, raw.previous_version, raw.target_version)
                .map_err(de::Error::custom)?;
        announcement.acknowledged = raw.acknowledged;
        Ok(announcement)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnnouncement {
    session_id: SessionId,
    previous_version: StableVersion,
    target_version: StableVersion,
    acknowledged: bool,
}

/// Durable release-highlight announcement validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReleaseHighlightAnnouncementError {
    /// Target is not strictly newer than the previous version.
    #[error("release highlight announcement version range is invalid")]
    InvalidVersionRange,
}

/// One exact stable release and its bounded user-facing highlights.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleaseHighlightGroup {
    version: StableVersion,
    highlights: Vec<String>,
}

impl ReleaseHighlightGroup {
    /// Exact stable version represented by this group.
    #[must_use]
    pub const fn version(&self) -> &StableVersion {
        &self.version
    }

    /// Reviewed concise highlights in display order.
    #[must_use]
    pub fn highlights(&self) -> &[String] {
        &self.highlights
    }
}

/// Complete validated packaged release-highlight catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseHighlightsManifest {
    releases: Vec<ReleaseHighlightGroup>,
}

impl ReleaseHighlightsManifest {
    /// Parse and validate one exact schema version from machine-readable JSON.
    ///
    /// # Errors
    ///
    /// Rejects oversized, malformed, noncanonical, unbounded, duplicate, or unordered input.
    pub fn parse_json(value: &str) -> Result<Self, ReleaseHighlightsError> {
        if value.len() > RELEASE_HIGHLIGHTS_MAX_BYTES {
            return Err(ReleaseHighlightsError::TooLarge);
        }
        let raw: RawManifest =
            serde_json::from_str(value).map_err(|_| ReleaseHighlightsError::Malformed)?;
        if raw.schema_version != 1 || raw.releases.is_empty() {
            return Err(ReleaseHighlightsError::UnsupportedSchema);
        }
        let mut releases = Vec::with_capacity(raw.releases.len());
        for raw_group in raw.releases {
            let version = StableVersion::parse(&raw_group.version)
                .map_err(|_| ReleaseHighlightsError::InvalidVersion)?;
            validate_highlights(&raw_group.highlights)?;
            if releases
                .last()
                .is_some_and(|previous: &ReleaseHighlightGroup| previous.version >= version)
            {
                return Err(ReleaseHighlightsError::UnorderedVersions);
            }
            releases.push(ReleaseHighlightGroup {
                version,
                highlights: raw_group.highlights,
            });
        }
        Ok(Self { releases })
    }

    /// All represented releases in strictly ascending version order.
    #[must_use]
    pub fn releases(&self) -> &[ReleaseHighlightGroup] {
        &self.releases
    }

    /// Exact installed release for an explicit manual reopen.
    #[must_use]
    pub fn installed(&self, version: &StableVersion) -> Option<ReleaseHighlightGroup> {
        self.releases
            .iter()
            .find(|group| group.version() == version)
            .cloned()
    }

    /// Packaged groups newer than `previous` through the exact represented `target`.
    #[must_use]
    pub fn between(
        &self,
        previous: &StableVersion,
        target: &StableVersion,
    ) -> Option<Vec<ReleaseHighlightGroup>> {
        if previous >= target || self.installed(target).is_none() {
            return None;
        }
        let groups = self
            .releases
            .iter()
            .filter(|group| group.version() > previous && group.version() <= target)
            .cloned()
            .collect::<Vec<_>>();
        (!groups.is_empty()).then_some(groups)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema_version: u32,
    releases: Vec<RawGroup>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGroup {
    version: String,
    highlights: Vec<String>,
}

fn validate_highlights(highlights: &[String]) -> Result<(), ReleaseHighlightsError> {
    if !(RELEASE_HIGHLIGHTS_MIN_ITEMS..=RELEASE_HIGHLIGHTS_MAX_ITEMS).contains(&highlights.len()) {
        return Err(ReleaseHighlightsError::InvalidItemCount);
    }
    for (index, highlight) in highlights.iter().enumerate() {
        if highlight.is_empty()
            || highlight.trim() != highlight
            || highlight.chars().count() > RELEASE_HIGHLIGHT_MAX_CHARS
            || highlight.chars().any(char::is_control)
            || highlights[..index].contains(highlight)
        {
            return Err(ReleaseHighlightsError::InvalidHighlight);
        }
    }
    Ok(())
}

/// Packaged release-highlight validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReleaseHighlightsError {
    /// Manifest exceeds the fixed product bound.
    #[error("release highlight manifest exceeds its size limit")]
    TooLarge,
    /// JSON shape or fields are malformed.
    #[error("release highlight manifest is malformed")]
    Malformed,
    /// Schema is unknown or contains no represented releases.
    #[error("release highlight manifest schema is unsupported")]
    UnsupportedSchema,
    /// A represented version is not exact stable semantic version syntax.
    #[error("release highlight version is invalid")]
    InvalidVersion,
    /// Versions are duplicated or not strictly ascending.
    #[error("release highlight versions are not strictly ascending")]
    UnorderedVersions,
    /// A represented release does not contain three through six highlights.
    #[error("release highlight count is outside its bounds")]
    InvalidItemCount,
    /// Highlight text is empty, padded, duplicated, multiline, controlled, or too long.
    #[error("release highlight text is invalid")]
    InvalidHighlight,
}

#[cfg(test)]
mod tests;
