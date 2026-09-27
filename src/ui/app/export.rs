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
        export::{ExistingFile, ExportOverwrite, ExportWriteRequest},
    },
};

use super::{BoardApp, pending_types::EditFlush, query::QueryEditor};
use crate::application::{DurabilityState, Effect};
use crate::ui::ListNavigation;

#[path = "export/choices.rs"]
mod choices;
mod completion;
mod finish;
mod input;
#[path = "export/view.rs"]
mod view;

use completion::{Candidate, CompletionRequest};
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
    /// First rendered row of the save row and completion choices.
    scroll: usize,
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
            && matches!(self.state.durability, DurabilityState::Failed { .. })
        {
            // No file is written for a Board change that storage cannot accept.
            self.set_storage_failure(
                "saving is failing; the thoughts cannot be removed or replaced now".to_owned(),
            );
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
        let mut field = QueryEditor::with_limit(MAX_PATH_BYTES);
        match self.export_base_directory().join(name).to_str() {
            Some(default) => field.paste(default),
            None => {
                self.set_info(
                    "the session folder is not valid UTF-8; type an absolute destination",
                );
            }
        }
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
            scroll: 0,
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
        self.request_write(ids, path, ExportOverwrite::Refuse)
    }

    /// Write the current copy text of the selected thoughts, read at save time.
    fn request_write(
        &mut self,
        ids: &mut impl IdGenerator,
        path: PathBuf,
        overwrite: ExportOverwrite,
    ) -> Vec<Effect> {
        let live = self.state.board.live_thoughts();
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        let current = state
            .sources
            .iter()
            .map(|source| {
                live.iter()
                    .find(|thought| thought.id == source.id)
                    .map(|thought| (*thought).clone())
            })
            .collect::<Option<Vec<_>>>();
        let Some(current) = current else {
            self.export.active = None;
            self.set_error("export: a selected thought was removed; nothing was written");
            return Vec::new();
        };
        state.sources = current;
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
        self.request_write(ids, path, ExportOverwrite::Confirmed(existing))
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
}

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
