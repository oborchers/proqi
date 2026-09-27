//! Render-only projection of the export overlay, shared by layout, rendering, and pointer rows.

use crate::domain::ExportDisposition;

use super::super::BoardApp;
use super::{ExportStage, ExportState};

/// Visible export overlay content.
pub(in crate::ui) enum ExportView {
    /// Destination field, the save row, and completion choices.
    Path {
        /// Overlay title.
        title: &'static str,
        /// Current field text.
        query: String,
        /// Save row followed by completion choices, from the first scrolled row.
        entries: Vec<String>,
        /// Highlighted row among `entries`.
        selected: usize,
    },
    /// Confirmation before replacing an existing file.
    Confirm {
        /// Overlay title.
        title: &'static str,
        /// Cancel, then Replace.
        entries: Vec<String>,
        /// Highlighted choice.
        selected: usize,
    },
}

impl ExportState {
    const fn title(&self) -> &'static str {
        match self.disposition {
            ExportDisposition::Keep => " export to file ",
            ExportDisposition::Remove => " export and remove ",
            ExportDisposition::ReplaceWithReference => " export and replace with reference ",
        }
    }

    fn save_label(&self) -> String {
        if matches!(self.stage, ExportStage::Writing { .. }) {
            return "Saving...".to_owned();
        }
        let count = self.sources.len();
        let noun = if count == 1 { "thought" } else { "thoughts" };
        match self.disposition {
            ExportDisposition::Keep => format!("Save {count} {noun} as plain text"),
            ExportDisposition::Remove => format!("Save and remove {count} {noun}"),
            ExportDisposition::ReplaceWithReference => {
                format!("Save and replace {count} {noun} with a file reference")
            }
        }
    }

    fn view(&self) -> ExportView {
        if let ExportStage::Confirm { path, selected, .. } = &self.stage {
            let file = path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
            return ExportView::Confirm {
                title: " replace existing file? ",
                entries: vec!["Cancel".to_owned(), format!("Replace {file}")],
                selected: *selected,
            };
        }
        let rows = std::iter::once(self.save_label()).chain(
            self.candidates
                .iter()
                .map(|candidate| candidate.label.clone()),
        );
        ExportView::Path {
            title: self.title(),
            query: self.field.text().to_owned(),
            entries: rows.skip(self.scroll).collect(),
            selected: self.highlighted_row().saturating_sub(self.scroll),
        }
    }

    pub(super) const fn row_count(&self) -> usize {
        match self.stage {
            ExportStage::Confirm { .. } => 2,
            ExportStage::Path | ExportStage::Writing { .. } => 1 + self.candidates.len(),
        }
    }
}

impl BoardApp {
    pub(in crate::ui) fn export_view(&self) -> Option<ExportView> {
        self.export.active.as_ref().map(ExportState::view)
    }

    pub(in crate::ui) fn export_row_count(&self) -> usize {
        self.export
            .active
            .as_ref()
            .map_or(0, ExportState::row_count)
    }

    pub(in crate::ui) fn export_field_cursor(&self) -> Option<usize> {
        self.export
            .active
            .as_ref()
            .map(|state| state.field.cursor())
    }

    pub(in crate::ui) fn export_field_selection(
        &self,
    ) -> Option<crate::ui::app::query::QuerySelection> {
        self.export
            .active
            .as_ref()
            .and_then(|state| state.field.selection())
    }
}
