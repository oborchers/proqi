//! Explicit text capture shared by `thoughts capture` and the Herdr plugin action.
//!
//! Both entrypoints store through one path: a live owner receives an exact add
//! that it appends and announces as a capture, and an inactive session commits
//! the same exact-content add under its lease, as `thoughts add` does.

use serde_json::json;

use crate::{
    application::{CaptureError, CaptureSource, CapturedText, ThoughtMutation, clipboard_text},
    domain::{OperationId, SessionId},
};

use super::{
    CliError, Outcome, RuntimeContext, forwarding, helpers::parse_operation_id, mutation_outcome,
    session_service,
};
use crate::cli::{args::CaptureFrom, error_code::ErrorCode};

/// Append exact captured text to one session as a new thought at the end.
pub(super) fn store(
    context: &mut RuntimeContext,
    session_id: SessionId,
    text: &str,
    operation: Option<OperationId>,
) -> Result<ThoughtMutation, CliError> {
    if let Some(result) = forwarding::capture(context, session_id, text, operation)? {
        return Ok(result);
    }
    Ok(session_service(context)?.add_thought(session_id, text.to_owned(), None, operation)?)
}

/// `proqi thoughts capture <session> --from clipboard`.
pub(super) fn thoughts_capture(
    context: &mut RuntimeContext,
    session: &str,
    from: CaptureFrom,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let operation = parse_operation_id(operation)?;
    let session_id = session_service(context)?.resolve_session(session, false)?;
    let text = match from {
        CaptureFrom::Clipboard => clipboard_text(&mut *context.capture_clipboard()),
    }
    .map_err(|error| capture_error(&error))?;
    let result = store(context, session_id, text.text(), operation)?;
    Ok(captured_outcome(
        mutation_outcome(result.thought_id, result.receipt),
        &text,
    ))
}

/// Add the capture's source and size to a stored thought's outcome.
pub(super) fn captured_outcome(mut outcome: Outcome, text: &CapturedText) -> Outcome {
    outcome.data["source"] = json!(text.source().as_str());
    outcome.data["characters"] = json!(text.characters());
    outcome.data["bytes"] = json!(text.text().len());
    outcome.human = format!("{}\n{}", text.confirmation(), outcome.human);
    outcome
}

/// Stable CLI classification of a capture that stored nothing.
pub(super) fn capture_error(error: &CaptureError) -> CliError {
    let message = error.to_string();
    match error {
        CaptureError::Empty { source } => CliError::new(ErrorCode::CaptureEmpty, message)
            .with_details(json!({ "source": source.as_str() })),
        CaptureError::NoText => CliError::new(ErrorCode::CaptureNoText, message)
            .with_details(json!({ "source": CaptureSource::Clipboard.as_str() })),
        CaptureError::TooLarge { source, bytes } => {
            CliError::new(ErrorCode::CaptureTooLarge, message).with_details(json!({
                "source": source.as_str(),
                "bytes": bytes,
                "limit": crate::application::MAX_THOUGHT_INPUT_BYTES,
            }))
        }
        CaptureError::Clipboard(_) => CliError::new(ErrorCode::ClipboardFailed, message),
    }
}
