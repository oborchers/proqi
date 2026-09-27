//! Destination completion flow: listing requests, offered choices, cycling, and scrolling.
//!
//! Row 0 of the path overlay is the save row; completion choices follow it. The
//! overlay scrolls over those rows so keyboard cycling and pointer rows reach the
//! same choices, and the highlighted choice always stays visible.

use crate::{
    application::Effect,
    ports::export::{DirectoryListing, DirectoryListingError},
};

use super::super::BoardApp;
use super::completion::{self, Candidate, CompletionOutcome};
use super::{ExportStage, ExportState, PendingListing};

impl BoardApp {
    /// Apply one generation-matched directory listing to the destination field.
    pub(crate) fn complete_export_listing(
        &mut self,
        generation: u64,
        result: Result<DirectoryListing, DirectoryListingError>,
    ) {
        let Some(state) = self.export.active.as_mut() else {
            return;
        };
        let Some(pending) = state
            .pending_listing
            .take_if(|pending| pending.generation == generation)
        else {
            return;
        };
        if state.field.text() != pending.text || !matches!(state.stage, ExportStage::Path) {
            return;
        }
        let listing = match result {
            Ok(listing) => listing,
            Err(error) => {
                self.set_warning(format!("no completions: {error}"));
                return;
            }
        };
        let matches = match completion::complete(&pending.request, &listing) {
            CompletionOutcome::NoMatch => {
                self.set_info("no matching file or folder");
                return;
            }
            CompletionOutcome::TooLarge => {
                self.set_info(
                    "this folder is too large to complete; type more of the name or all of it",
                );
                return;
            }
            CompletionOutcome::Completed {
                text,
                candidates,
                matches,
            } => {
                state.replace_field(&text);
                state.offer(candidates);
                matches
            }
            CompletionOutcome::Ambiguous {
                candidates,
                matches,
            } => {
                state.offer(candidates);
                state.cycle(pending.backward);
                matches
            }
        };
        let offered = state.candidates.len();
        self.layout = None;
        if matches > offered && offered > 0 {
            self.set_info(format!(
                "showing {offered} of {matches} matches; type more to narrow them"
            ));
        }
    }

    /// Request a listing, or cycle through the choices of the previous listing.
    pub(super) fn complete_export_field(&mut self, backward: bool) -> Vec<Effect> {
        let home = self.export.home_directory.clone();
        let base = self.export_base_directory();
        self.export.generation = self.export.generation.wrapping_add(1);
        let generation = self.export.generation;
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        if !matches!(state.stage, ExportStage::Path) {
            return Vec::new();
        }
        if state.cycling_matches_field() {
            state.cycle(backward);
            self.layout = None;
            return Vec::new();
        }
        if state.field.text() == "~" {
            state.replace_field("~/");
            return Vec::new();
        }
        let Some(request) =
            completion::completion_request(state.field.text(), &base, home.as_deref())
        else {
            self.set_warning("no completions: the home directory is unknown");
            return Vec::new();
        };
        let directory = request.directory.clone();
        let prefix = request.partial.clone();
        state.pending_listing = Some(PendingListing {
            generation,
            text: state.field.text().to_owned(),
            request,
            backward,
        });
        vec![Effect::ListExportDirectory {
            generation,
            directory,
            prefix,
        }]
    }

    /// Move one offered choice, exactly as `Tab` and `Shift+Tab` do. Shared by
    /// the arrow keys and the wheel; without offered choices nothing happens.
    pub(super) fn step_export_choices(&mut self, backward: bool) {
        if let Some(state) = self.export.active.as_mut()
            && state.navigable()
        {
            state.cycle(backward);
            self.layout = None;
        }
    }

    /// Move several offered choices at once, stopping at the first and last.
    pub(super) fn jump_export_choices(&mut self, delta: isize) {
        if let Some(state) = self.export.active.as_mut()
            && state.navigable()
        {
            let last = state.candidates.len() - 1;
            let steps = delta.unsigned_abs();
            // Before any choice is highlighted, the list starts just outside
            // either end, as it does for the first Tab or Shift+Tab.
            let target = match state.cycled {
                Some(index) => index.saturating_add_signed(delta).min(last),
                None if delta > 0 => steps.saturating_sub(1).min(last),
                None => (last + 1).saturating_sub(steps),
            };
            state.select(target);
            self.layout = None;
        }
    }

    /// Choose a completion by its index among the offered choices.
    pub(super) fn apply_export_candidate(&mut self, index: usize) {
        if let Some(state) = self.export.active.as_mut()
            && matches!(state.stage, ExportStage::Path)
            && let Some(candidate) = state.candidates.get(index).cloned()
        {
            state.replace_field(&candidate.text);
            state.forget_completion();
            self.layout = None;
        }
    }

    /// Keep the highlighted row inside `visible` rendered rows.
    pub(in crate::ui::app) fn ensure_export_visible(&mut self, visible: usize) {
        if let Some(state) = self.export.active.as_mut() {
            let last = state.row_count().saturating_sub(1);
            state.scroll = crate::ui::paging::first_visible(
                state.highlighted_row(),
                state.scroll.min(last),
                visible,
            );
        }
    }

    /// Whether rows are hidden above or below the `visible` rendered rows.
    pub(in crate::ui) fn export_overflow(&self, visible: usize) -> (bool, bool) {
        self.export.active.as_ref().map_or((false, false), |state| {
            (
                state.scroll > 0,
                state.scroll.saturating_add(visible) < state.row_count(),
            )
        })
    }

    /// Absolute row index for a rendered row.
    pub(super) fn export_row_at(&self, visible_index: usize) -> usize {
        self.export.active.as_ref().map_or(visible_index, |state| {
            if matches!(state.stage, ExportStage::Confirm { .. }) {
                visible_index
            } else {
                state.scroll.saturating_add(visible_index)
            }
        })
    }
}

impl ExportState {
    /// Drop offered choices once the typed text no longer comes from them.
    pub(super) fn forget_completion(&mut self) {
        self.candidates.clear();
        self.cycled = None;
        self.pending_listing = None;
        self.scroll = 0;
    }

    /// Keep the offered choices together with the text they complete.
    fn offer(&mut self, candidates: Vec<Candidate>) {
        self.candidates = candidates;
        self.anchor = self.field.text().to_owned();
        self.cycled = None;
        self.scroll = 0;
    }

    /// Whether Tab should move through the offered choices instead of listing again.
    fn cycling_matches_field(&self) -> bool {
        let current = self.cycled.map_or(self.anchor.as_str(), |index| {
            self.candidates
                .get(index)
                .map_or("", |candidate| candidate.text.as_str())
        });
        !self.candidates.is_empty() && self.field.text() == current
    }

    /// Whether offered choices can be moved through without listing again.
    fn navigable(&self) -> bool {
        matches!(self.stage, ExportStage::Path) && self.cycling_matches_field()
    }

    fn select(&mut self, index: usize) {
        if let Some(candidate) = self.candidates.get(index) {
            let text = candidate.text.clone();
            self.cycled = Some(index);
            self.replace_field(&text);
        }
    }

    fn cycle(&mut self, backward: bool) {
        let count = self.candidates.len();
        if count == 0 {
            return;
        }
        let next = match (self.cycled, backward) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(index), false) => (index + 1) % count,
            (Some(index), true) => (index + count - 1) % count,
        };
        self.select(next);
    }

    /// Highlighted row: the save row, or the cycled choice after it.
    pub(super) fn highlighted_row(&self) -> usize {
        self.cycled.map_or(0, |index| index + 1)
    }
}
