//! Compile-time packaged release-highlight manifest.

use crate::domain::{ReleaseHighlightsError, ReleaseHighlightsManifest};

/// Parse the exact manifest bytes embedded in this executable.
///
/// # Errors
///
/// Returns validation failure without falling back to network or external files.
pub fn packaged() -> Result<ReleaseHighlightsManifest, ReleaseHighlightsError> {
    ReleaseHighlightsManifest::parse_json(include_str!("../../../release-highlights.json"))
}
