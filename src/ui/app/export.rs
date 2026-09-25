//! Export selected thoughts to a plain-text file: path field, completion, confirmation.
//!
//! The field reuses the shared single-line [`QueryEditor`]. Writing and directory
//! listing run on the external lane; the Board changes only after the file is durable.

use std::path::PathBuf;

use crate::{
    application::copy_text,
    domain::{ExportDisposition, Thought, default_export_file_name, resolve_export_path},
    ports::{
        environment::{Clock, IdGenerator},
        export::{
            DirectoryListing, DirectoryListingError, ExistingFile, ExportOverwrite,
            ExportWriteRequest,
        },
    },
};

use super::{BoardApp, pending_types::EditFlush, query::QueryEditor};
use crate::application::Effect;
use crate::ui::ListNavigation;

mod completion;
mod finish;
mod input;
#[path = "export/view.rs"]
mod view;

use completion::{Candidate, CompletionOutcome, CompletionRequest};
pub(in crate::ui) use view::ExportView;

/// Largest destination text accepted by the field.
const MAX_PATH_BYTES: usize = 4 * 1024;

/// Export overlay state plus the values that outlive one overlay.
#[derive(Default)]
pub(super) struct ExportOwner {
    /// The open export overlay, if any.
    pub(super) active: Option<ExportState>,
    /// Latest directory-listing generation.
    pub(super) generation: u64,
    /// Home directory for `~` in destinations.
    pub(super) home_directory: Option<PathBuf>,
}

pub(super) struct ExportState {
    disposition: ExportDisposition,
    sources: Vec<Thought>,
    field: QueryEditor,
    candidates: Vec<Candidate>,
    anchor: String,
    cycled: Option<usize>,
    pending_listing: Option<PendingListing>,
    stage: ExportStage,
}

struct PendingListing {
    generation: u64,
    text: String,
    request: CompletionRequest,
    backward: bool,
}

enum ExportStage {
    Path,
    Writing {
        request_id: crate::domain::RequestId,
        path: PathBuf,
    },
    Confirm {
        path: PathBuf,
        existing: ExistingFile,
        selected: usize,
    },
}

impl ExportState {
    pub(super) const fn input_owner(&self) -> super::input_dispatch::ActiveInputOwner {
        match self.stage {
            ExportStage::Path | ExportStage::Writing { .. } => {
                super::input_dispatch::ActiveInputOwner::ExportPath
            }
            ExportStage::Confirm { .. } => super::input_dispatch::ActiveInputOwner::ExportReplace,
        }
    }

    /// Whether a durable write may still remove or replace thoughts.
    pub(super) const fn board_change_pending(&self) -> bool {
        self.disposition.changes_board() && matches!(self.stage, ExportStage::Writing { .. })
    }
}

impl BoardApp {
    /// Set the home directory used to resolve `~` in export destinations.
    pub fn set_home_directory(&mut self, home: Option<PathBuf>) {
        self.export.home_directory = home.filter(|home| home.is_absolute());
    }

    pub(super) fn export_input_owner(&self) -> Option<super::input_dispatch::ActiveInputOwner> {
        self.export.active.as_ref().map(ExportState::input_owner)
    }

    /// Open the destination field for the current selection or focused thought.
    pub(super) fn begin_export(
        &mut self,
        disposition: ExportDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.deactivate_range_latch();
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            if self.action_has_separator() {
                self.set_info("separator has no text to export");
            } else {
                self.set_warning("select a thought before exporting it");
            }
            return Vec::new();
        }
        if disposition.changes_board()
            && (self.state.deferred_board_operation_pending()
                || thought_ids.iter().any(|id| !self.thought_mutable(*id)))
        {
            self.set_warning("selected thought has an operation in progress");
            return Vec::new();
        }
        let effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let sources = self
            .state
            .board
            .live_thoughts()
            .into_iter()
            .filter(|thought| thought_ids.contains(&thought.id))
            .cloned()
            .collect::<Vec<_>>();
        if sources.len() != thought_ids.len() {
            self.set_warning("board changed before export; nothing was written");
            return effects;
        }
        let single_name = match sources.as_slice() {
            [only] => only.name.as_ref(),
            _ => None,
        };
        let name = default_export_file_name(single_name, self.session_display_name(), clock.now());
        let default = self.export_base_directory().join(name);
        let mut field = QueryEditor::with_limit(MAX_PATH_BYTES);
        field.paste(&default.to_string_lossy());
        self.help = false;
        self.palette = None;
        self.export.active = Some(ExportState {
            disposition,
            sources,
            field,
            candidates: Vec::new(),
            anchor: String::new(),
            cycled: None,
            pending_listing: None,
            stage: ExportStage::Path,
        });
        self.layout = None;
        effects
    }

    /// The session's most recent opening directory, the base for relative destinations.
    fn export_base_directory(&self) -> PathBuf {
        self.state.board.session.last_opened_cwd.clone()
    }

    pub(super) fn cancel_export(&mut self) {
        if self
            .export
            .active
            .as_ref()
            .is_some_and(|state| matches!(state.stage, ExportStage::Writing { .. }))
        {
            return;
        }
        self.export.active = None;
        self.set_info("export cancelled");
    }

    /// Resolve the typed destination and request one atomic write.
    fn commit_export_path(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        let home = self.export.home_directory.clone();
        let base = self.export_base_directory();
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        if !matches!(state.stage, ExportStage::Path) {
            return Vec::new();
        }
        let path = match resolve_export_path(state.field.text(), &base, home.as_deref()) {
            Ok(path) => path,
            Err(error) => {
                self.set_error(format!("export: {error}"));
                return Vec::new();
            }
        };
        if path.to_str().is_none() {
            self.set_error("export: the path must be valid UTF-8");
            return Vec::new();
        }
        Self::request_write(state, ids, path, ExportOverwrite::Refuse)
    }

    fn request_write(
        state: &mut ExportState,
        ids: &mut impl IdGenerator,
        path: PathBuf,
        overwrite: ExportOverwrite,
    ) -> Vec<Effect> {
        let request_id = ids.request_id();
        state.stage = ExportStage::Writing {
            request_id,
            path: path.clone(),
        };
        state.candidates.clear();
        state.cycled = None;
        state.pending_listing = None;
        vec![Effect::WriteExport {
            request_id,
            request: ExportWriteRequest {
                path,
                content: copy_text(state.sources.iter()),
                overwrite,
            },
        }]
    }

    fn choose_export_replacement(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        let ExportStage::Confirm {
            path,
            existing,
            selected,
        } = &state.stage
        else {
            return Vec::new();
        };
        if *selected == 0 {
            state.stage = ExportStage::Path;
            self.set_info("existing file kept; choose another name");
            return Vec::new();
        }
        let (path, existing) = (path.clone(), *existing);
        Self::request_write(state, ids, path, ExportOverwrite::Confirmed(existing))
    }

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
        match completion::complete(&pending.request, &listing) {
            CompletionOutcome::NoMatch => self.set_info("no matching file or folder"),
            CompletionOutcome::Completed { text, candidates } => {
                state.replace_field(&text);
                state.offer(candidates);
            }
            CompletionOutcome::Ambiguous(candidates) => {
                state.offer(candidates);
                state.cycle(pending.backward);
            }
        }
    }

    /// Request a listing, or cycle through the choices of the previous listing.
    fn complete_export_field(&mut self, backward: bool) -> Vec<Effect> {
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
        state.pending_listing = Some(PendingListing {
            generation,
            text: state.field.text().to_owned(),
            request,
            backward,
        });
        vec![Effect::ListExportDirectory {
            generation,
            directory,
        }]
    }

    /// Choose a completion row by pointer.
    fn apply_export_candidate(&mut self, index: usize) {
        if let Some(state) = self.export.active.as_mut()
            && matches!(state.stage, ExportStage::Path)
            && let Some(candidate) = state.candidates.get(index).cloned()
        {
            state.replace_field(&candidate.text);
            state.candidates.clear();
            state.cycled = None;
        }
    }

    fn move_export_choice(&mut self, navigation: ListNavigation) {
        if let Some(ExportState {
            stage: ExportStage::Confirm { selected, .. },
            ..
        }) = self.export.active.as_mut()
        {
            *selected = usize::from(navigation == ListNavigation::Next);
        }
    }
}

impl ExportState {
    fn replace_field(&mut self, text: &str) {
        self.field.select_all();
        self.field.paste(text);
    }

    /// Drop offered choices once the typed text no longer comes from them.
    fn forget_completion(&mut self) {
        self.candidates.clear();
        self.cycled = None;
        self.pending_listing = None;
    }

    /// Keep the offered choices together with the text they complete.
    fn offer(&mut self, candidates: Vec<Candidate>) {
        self.candidates = candidates;
        self.anchor = self.field.text().to_owned();
        self.cycled = None;
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
        self.cycled = Some(next);
        let text = self.candidates[next].text.clone();
        self.replace_field(&text);
    }
}

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
