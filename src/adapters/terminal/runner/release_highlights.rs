//! Quiet startup selection of packaged highlights after board restoration.

use std::path::Path;

use crate::{
    adapters::update::{FileUpdateStateStore, packaged_release_highlights},
    application::ReleaseHighlightSelection,
    domain::{Installation, SessionId, StableVersion},
    ports::update::UpdateStateStore as _,
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
