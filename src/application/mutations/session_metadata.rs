//! Reversible session metadata mutations owned by Browser history.

use crate::{
    application::{AppState, Effect, error::ApplicationResult},
    domain::{BrowserOperation, OperationId, Timestamp},
};

pub(in crate::application) fn rename_session(
    state: &mut AppState,
    operation_id: OperationId,
    name: Option<String>,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let previous_name = state.board.session.name.clone();
    state.board.session.rename(name.clone())?;
    if previous_name == name {
        return Ok(Vec::new());
    }
    let session_id = state.board.session.id;
    Ok(vec![Effect::CommitBrowserOperation(
        BrowserOperation::rename(operation_id, session_id, previous_name, name, at)?,
    )])
}
