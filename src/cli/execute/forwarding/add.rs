//! Active-owner forwarding for exact thought creation: plain, preserving, and
//! announced captures share one request builder.

use crate::{
    application::ThoughtMutation,
    domain::{ContentAnnotation, OperationId, SessionId, ThoughtId},
    ports::{
        control::{AddAnnouncement, ControlMutation},
        environment::IdGenerator,
    },
};

use super::{owner, send};
use crate::cli::{output::CliError, runtime::RuntimeContext};

struct ForwardedAdd<'a> {
    body: &'a str,
    annotations: Vec<ContentAnnotation>,
    name: Option<crate::domain::ThoughtName>,
    position: Option<usize>,
    supplied: Option<OperationId>,
    preserve: bool,
    announcement: Option<AddAnnouncement>,
}

pub(in crate::cli::execute) fn add(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_with_kind(
        context,
        session_id,
        ForwardedAdd {
            body,
            annotations: Vec::new(),
            name: None,
            position,
            supplied,
            preserve: false,
            announcement: None,
        },
    )
}

/// Forward one explicit capture: an exact add appended and announced by the owner.
pub(in crate::cli::execute) fn capture(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_with_kind(
        context,
        session_id,
        ForwardedAdd {
            body,
            annotations: Vec::new(),
            name: None,
            position: None,
            supplied,
            preserve: false,
            announcement: Some(AddAnnouncement::Capture),
        },
    )
}

pub(in crate::cli::execute) fn preserve_add(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    annotations: Vec<ContentAnnotation>,
    name: Option<crate::domain::ThoughtName>,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_with_kind(
        context,
        session_id,
        ForwardedAdd {
            body,
            annotations,
            name,
            position,
            supplied,
            preserve: true,
            announcement: None,
        },
    )
}

fn add_with_kind(
    context: &mut RuntimeContext,
    session_id: SessionId,
    add: ForwardedAdd<'_>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = add.supplied.unwrap_or_else(|| context.ids.operation_id());
    let thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|error| CliError::identifier(error.to_string()))?;
    let mutation = if add.preserve {
        ControlMutation::PreserveAdd {
            operation_id,
            thought_id,
            content: add.body.to_owned(),
            annotations: add.annotations,
            name: add.name,
            position: add.position,
        }
    } else {
        ControlMutation::Add {
            operation_id,
            thought_id,
            content: add.body.to_owned(),
            annotations: Vec::new(),
            position: add.position,
            announcement: add.announcement,
        }
    };
    let receipt = send(context, &owner, session_id, mutation)?;
    Ok(Some(ThoughtMutation {
        thought_id,
        receipt,
    }))
}
