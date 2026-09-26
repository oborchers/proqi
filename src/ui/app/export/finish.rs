//! Durable write completion: confirmation, failure, and the one Board operation.

use crate::{
    application::{Action, Effect, EmptyBoardTransition, ExportBoardChange, ExportCompletion},
    domain::ExportDisposition,
    ports::{
        environment::{Clock, IdGenerator},
        export::{ExportWriteError, ExportWritten},
    },
};

use super::super::BoardApp;
use super::ExportStage;

impl BoardApp {
    /// Apply one atomic write result. The Board changes only after durable success.
    pub(crate) fn complete_export_write(
        &mut self,
        request_id: crate::domain::RequestId,
        result: Result<ExportWritten, ExportWriteError>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        let ExportStage::Writing {
            request_id: pending,
            path,
        } = &state.stage
        else {
            return Vec::new();
        };
        if *pending != request_id {
            return Vec::new();
        }
        let path = path.clone();
        match result {
            Ok(written) => self.finish_export(&written, ids, clock),
            Err(ExportWriteError::Exists(existing)) => {
                state.stage = ExportStage::Confirm {
                    path,
                    existing,
                    selected: 0,
                };
                Vec::new()
            }
            Err(error) => {
                state.stage = ExportStage::Path;
                self.set_error(format!("export: {error}; the board was not changed"));
                Vec::new()
            }
        }
    }

    fn finish_export(
        &mut self,
        written: &ExportWritten,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(state) = self.export.active.take() else {
            return Vec::new();
        };
        let file = written
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let count = state.sources.len();
        let (noun, them) = if count == 1 {
            ("thought", "it")
        } else {
            ("thoughts", "them")
        };
        let change = match state.disposition {
            ExportDisposition::Keep => {
                self.set_success(format!("exported {count} {noun} to {file}"));
                return Vec::new();
            }
            ExportDisposition::Remove => ExportBoardChange::Remove,
            ExportDisposition::ReplaceWithReference => {
                let operation_id = ids.operation_id();
                let reference_thought_id = ids.thought_id();
                return self.apply_export_completion(
                    ExportCompletion {
                        operation_id,
                        thought_ids: state.sources.iter().map(|thought| thought.id).collect(),
                        expected_sources: state.sources,
                        change: ExportBoardChange::ReplaceWithReference {
                            reference_thought_id,
                            path: written.path.clone(),
                        },
                        at: clock.now(),
                    },
                    format!(
                        "exported {count} {noun} to {file} and replaced {them} with a reference"
                    ),
                );
            }
        };
        let completion = ExportCompletion {
            operation_id: ids.operation_id(),
            thought_ids: state.sources.iter().map(|thought| thought.id).collect(),
            expected_sources: state.sources,
            change,
            at: clock.now(),
        };
        self.apply_export_completion(
            completion,
            format!("exported {count} {noun} to {file} and removed {them}"),
        )
    }

    fn apply_export_completion(
        &mut self,
        completion: ExportCompletion,
        success: String,
    ) -> Vec<Effect> {
        let Some(effects) = self.reduce_with_empty_transition_described(
            Action::CompleteExport(completion),
            EmptyBoardTransition::ComposeAfterLocalRemoval,
            |cause| format!("file saved, but the board was kept: {cause}"),
        ) else {
            return Vec::new();
        };
        self.set_success(success);
        effects
    }
}
