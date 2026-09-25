//! Inactive-session Board completion after a durable plain-text export.

use super::{BoardItemMutation, SessionService, SessionServiceError};
use crate::{
    application::{Action, export_completion_for_request},
    domain::SessionId,
    ports::{
        control::ControlMutation,
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::Store,
    },
};

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Remove or replace exported thoughts as one Board operation under the session lease.
    ///
    /// The caller must already have written and synchronized the export file. An exact
    /// retry of the same operation identity replays the original receipt.
    ///
    /// # Errors
    ///
    /// Returns a typed precondition, idempotency, lease, or persistence failure.
    pub fn complete_export(
        &mut self,
        session_id: SessionId,
        request: &ControlMutation,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        if !matches!(request, ControlMutation::ExportThoughts { .. }) {
            return Err(crate::application::ApplicationError::InvalidState.into());
        }
        self.apply_transform(session_id, request, |state, at| {
            Ok(Action::CompleteExport(export_completion_for_request(
                state, request, at,
            )?))
        })
    }
}
