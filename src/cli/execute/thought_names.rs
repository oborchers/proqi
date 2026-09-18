//! Thought-name CLI mutation through inactive or active session ownership.

use super::{
    CliError, Outcome, RuntimeContext, forwarding,
    helpers::{parse_operation_id, parse_thought_id},
    mutation_outcome, session_service,
};

pub(super) fn rename_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    name: Option<&str>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let name = match name.map(str::trim) {
        None | Some("") => None,
        Some(value) => Some(
            crate::domain::ThoughtName::new(value.to_owned())
                .map_err(|error| CliError::arguments(error.to_string()))?,
        ),
    };
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) =
        forwarding::rename_thought(context, session_id, thought_id, name.clone(), operation)?
    {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.rename_thought(session_id, thought_id, name, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}
