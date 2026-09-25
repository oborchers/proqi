//! `thoughts export`: durable plain-text file first, then one optional Board operation.

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::{
    adapters::{export::FileExport, runtime::SystemEnvironment},
    application::{
        BoardItemMutation, ControlReplay, copy_text, export_reference_thought_id,
        match_control_replay,
    },
    cli::{error_code::ErrorCode, runtime::RuntimeContext},
    domain::{
        ExportDisposition, ExportPathError, SessionId, Thought, ThoughtId, resolve_export_path,
    },
    ports::{
        control::ControlMutation,
        environment::{Environment as _, IdGenerator as _},
        export::{ExportOverwrite, ExportWriteError, ExportWriteRequest, ExportWriter as _},
        store::Store as _,
    },
};

use super::{
    CliError, Outcome, forwarding,
    helpers::{parse_operation_id, parse_thought_id},
    session_service,
};

/// Parsed `thoughts export` request.
pub(super) struct ExportArguments<'a> {
    pub(super) session: &'a str,
    pub(super) thoughts: &'a [String],
    pub(super) output: &'a str,
    pub(super) disposition: ExportDisposition,
    pub(super) replace_existing: bool,
    pub(super) operation: Option<&'a str>,
}

pub(super) fn export(
    context: &mut RuntimeContext,
    arguments: &ExportArguments<'_>,
) -> Result<Outcome, CliError> {
    let thought_ids = parse_unique_thoughts(arguments.thoughts)?;
    let operation = parse_operation_id(arguments.operation)?;
    let path = resolve_export_path(
        arguments.output,
        &context.cwd,
        SystemEnvironment.home_directory().as_deref(),
    )
    .map_err(|error| invalid_output(arguments.output, error))?;
    let output = path
        .to_str()
        .ok_or_else(|| {
            CliError::new(
                ErrorCode::ExportTargetInvalid,
                "the destination path must be valid UTF-8".to_owned(),
            )
            .with_details(json!({ "output": arguments.output, "reason": "not_utf8" }))
        })?
        .to_owned();
    let session_id = session_service(context)?.resolve_session(arguments.session, false)?;
    forwarding::sync(context, session_id)?;
    let sources = ordered_sources(context, session_id, &thought_ids)?;
    let text = copy_text(sources.iter());
    let operation_id = operation.unwrap_or_else(|| context.ids.operation_id());
    let request = board_request(arguments.disposition, operation_id, &sources, &output)?;
    let all_live = sources.iter().all(Thought::is_live);
    if let Some(request) = &request
        && operation.is_some()
        && let Some(replayed) = replay(context, session_id, request)?
    {
        return Ok(outcome(
            &output,
            &text,
            arguments.disposition,
            &sources,
            false,
            false,
            Some(&replayed),
        ));
    }
    if !all_live {
        let missing = sources
            .iter()
            .find(|thought| !thought.is_live())
            .map_or_else(|| thought_ids[0], |thought| thought.id);
        return Err(CliError::new(
            ErrorCode::ThoughtNotFound,
            format!("thought not found: {missing}"),
        ));
    }
    let overwrite = if arguments.replace_existing {
        ExportOverwrite::Always
    } else if operation.is_some() {
        ExportOverwrite::RefuseUnlessIdentical
    } else {
        ExportOverwrite::Refuse
    };
    let written = FileExport
        .write(&ExportWriteRequest {
            path,
            content: text.clone(),
            overwrite,
        })
        .map_err(|error| write_error(&output, &error))?;
    let receipt = match &request {
        Some(request) => Some(commit(context, session_id, request).map_err(|error| {
            error.with_details(json!({ "output": output, "file_written": true }))
        })?),
        None => None,
    };
    Ok(outcome(
        &output,
        &text,
        arguments.disposition,
        &sources,
        !written.unchanged,
        written.replaced,
        receipt.as_ref(),
    ))
}

fn parse_unique_thoughts(values: &[String]) -> Result<Vec<ThoughtId>, CliError> {
    let mut thought_ids = Vec::with_capacity(values.len());
    for value in values {
        let thought_id = parse_thought_id(value)?;
        if thought_ids.contains(&thought_id) {
            return Err(CliError::arguments(format!(
                "thought listed more than once: {thought_id}"
            )));
        }
        thought_ids.push(thought_id);
    }
    Ok(thought_ids)
}

/// Load the requested thoughts, live or recoverably deleted, in their Board order.
fn ordered_sources(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_ids: &[ThoughtId],
) -> Result<Vec<Thought>, CliError> {
    let snapshot = session_service(context)?.inspect_session(session_id)?;
    let mut sources = thought_ids
        .iter()
        .map(|thought_id| {
            snapshot.board.thought(*thought_id).cloned().ok_or_else(|| {
                CliError::new(
                    ErrorCode::ThoughtNotFound,
                    format!("thought not found: {thought_id}"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    sources.sort_by_key(|thought| thought.position.get());
    Ok(sources)
}

fn board_request(
    disposition: ExportDisposition,
    operation_id: crate::domain::OperationId,
    sources: &[Thought],
    output: &str,
) -> Result<Option<ControlMutation>, CliError> {
    if !disposition.changes_board() {
        return Ok(None);
    }
    let replace = matches!(disposition, ExportDisposition::ReplaceWithReference);
    let reference_thought_id = if replace {
        Some(export_reference_thought_id(operation_id).map_err(|_| {
            CliError::identifier("operation ID cannot derive a thought identity".to_owned())
        })?)
    } else {
        None
    };
    Ok(Some(ControlMutation::ExportThoughts {
        operation_id,
        thought_ids: sources.iter().map(|thought| thought.id).collect(),
        expected_digests: sources
            .iter()
            .map(|thought| Sha256::digest(thought.content.as_bytes()).into())
            .collect(),
        disposition,
        reference_thought_id,
        output_path: output.to_owned(),
    }))
}

fn replay(
    context: &mut RuntimeContext,
    session_id: SessionId,
    request: &ControlMutation,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(operation_id) = request.durable_operation_id() else {
        return Ok(None);
    };
    let Some(existing) = context.store.operation_request(operation_id)? else {
        return Ok(None);
    };
    match match_control_replay(&existing, session_id, request) {
        ControlReplay::Accepted(receipt) => Ok(Some(BoardItemMutation {
            item_ids: receipt.item_ids,
            receipt: receipt.durable,
        })),
        ControlReplay::Conflict => Err(CliError::new(
            ErrorCode::IdempotencyConflict,
            "operation ID belongs to another mutation".to_owned(),
        )),
    }
}

fn commit(
    context: &mut RuntimeContext,
    session_id: SessionId,
    request: &ControlMutation,
) -> Result<BoardItemMutation, CliError> {
    if let Some(result) = forwarding::export_thoughts(context, session_id, request.clone())? {
        return Ok(result);
    }
    Ok(session_service(context)?.complete_export(session_id, request)?)
}

fn outcome(
    output: &str,
    content: &str,
    disposition: ExportDisposition,
    sources: &[Thought],
    written: bool,
    replaced: bool,
    board: Option<&BoardItemMutation>,
) -> Outcome {
    let reference = board.and_then(|result| {
        matches!(disposition, ExportDisposition::ReplaceWithReference)
            .then(|| result.item_ids.first().and_then(|item| item.thought()))
            .flatten()
    });
    let receipt = board.map_or(Value::Null, |result| {
        super::receipt_outcome(result.receipt).data["receipt"].clone()
    });
    let count = sources.len();
    let action = match disposition {
        ExportDisposition::Keep => "Exported",
        ExportDisposition::Remove => "Exported and removed",
        ExportDisposition::ReplaceWithReference => "Exported and replaced",
    };
    Outcome {
        data: json!({
            "output": output,
            "bytes": content.len(),
            "disposition": disposition.as_str(),
            "thought_ids": sources.iter().map(|thought| thought.id).collect::<Vec<_>>(),
            "written": written,
            "replaced": replaced,
            "reference_thought_id": reference,
            "item_ids": board.map_or_else(Vec::new, |result| result.item_ids.clone()),
            "receipt": receipt,
        }),
        human: format!(
            "{action} {count} thought{} to {output}",
            if count == 1 { "" } else { "s" }
        ),
    }
}

fn invalid_output(output: &str, error: ExportPathError) -> CliError {
    let reason = match error {
        ExportPathError::Empty => "empty",
        ExportPathError::HomeUnavailable => "home_unavailable",
        ExportPathError::MissingFileName => "missing_file_name",
        ExportPathError::RelativeBase => "relative_base",
    };
    CliError::new(ErrorCode::ExportTargetInvalid, error.to_string())
        .with_details(json!({ "output": output, "reason": reason }))
}

fn write_error(output: &str, error: &ExportWriteError) -> CliError {
    let code = match error {
        ExportWriteError::Exists(_) => {
            return CliError::new(
                ErrorCode::ExportTargetExists,
                format!("a file already exists at {output}; pass --replace-existing to replace it"),
            )
            .with_details(json!({ "output": output }));
        }
        ExportWriteError::DirectoryMissing | ExportWriteError::ParentNotDirectory => {
            ErrorCode::ExportDirectoryMissing
        }
        ExportWriteError::InvalidPath
        | ExportWriteError::TargetIsDirectory
        | ExportWriteError::TargetIsSymlink
        | ExportWriteError::TargetNotRegular => ErrorCode::ExportTargetInvalid,
        ExportWriteError::Changed
        | ExportWriteError::ReplaceUnsupported
        | ExportWriteError::Displaced(_)
        | ExportWriteError::PermissionDenied
        | ExportWriteError::ReadOnly
        | ExportWriteError::StorageFull
        | ExportWriteError::Io => ErrorCode::ExportWriteFailed,
    };
    CliError::new(code, format!("{error}: {output}"))
        .with_details(json!({ "output": output, "reason": error.reason() }))
}
