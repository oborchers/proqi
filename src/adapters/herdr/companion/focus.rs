//! Focus a pane by identity while preserving the tab's zoom state.
//!
//! Herdr 0.8.0 through 0.9.1 focus a pane directly only when the plugin owns it
//! or it is a recognized agent. Any other pane is focused by zooming it. The
//! tab's zoom state is read first and restored exactly: an unzoomed tab is
//! zoomed and unzoomed again, and a zoomed tab keeps its zoom, moved to the
//! target pane.

use serde::Deserialize;

use crate::ports::environment::ProcessRunner;

use super::{HerdrCompanionHost, HostFailure, QUERY_TIMEOUT};

#[derive(Deserialize)]
struct LayoutBody {
    layout: LayoutZoom,
}

#[derive(Deserialize)]
struct LayoutZoom {
    zoomed: bool,
    focused_pane_id: String,
}

impl<R: ProcessRunner> HerdrCompanionHost<R> {
    pub(super) fn focus_any(&mut self, pane_id: &str) -> Result<(), HostFailure> {
        match self.run(&["plugin", "pane", "focus", pane_id], QUERY_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(HostFailure::Rejected { .. }) => {}
            Err(error) => return Err(error),
        }
        let layout: LayoutBody =
            self.json(&["pane", "layout", "--pane", pane_id], QUERY_TIMEOUT)?;
        let LayoutZoom {
            zoomed,
            focused_pane_id,
        } = layout.layout;
        if focused_pane_id == pane_id {
            return Ok(());
        }
        if zoomed {
            self.run(&["pane", "zoom", &focused_pane_id, "--off"], QUERY_TIMEOUT)?;
            self.run(&["pane", "zoom", pane_id, "--on"], QUERY_TIMEOUT)
                .map(|_| ())
                .inspect_err(|_| {
                    // Put the user's zoom back before reporting the failure.
                    let _best_effort =
                        self.run(&["pane", "zoom", &focused_pane_id, "--on"], QUERY_TIMEOUT);
                })
        } else {
            self.run(&["pane", "zoom", pane_id, "--on"], QUERY_TIMEOUT)?;
            self.run(&["pane", "zoom", pane_id, "--off"], QUERY_TIMEOUT)
                .map(|_| ())
        }
    }
}
