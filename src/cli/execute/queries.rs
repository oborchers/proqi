//! Read-only thought projections for human and JSON callers.

use serde_json::json;

use crate::domain::BoardItemRef;

use super::{
    CliError, Outcome, forwarding,
    helpers::{content_digest_hex, excerpt, parse_thought_id},
    session_service,
};
use crate::cli::runtime::RuntimeContext;

pub(super) fn list(context: &mut RuntimeContext, reference: &str) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(reference, true)?;
    drop(service);
    forwarding::sync(context, session_id)?;
    let mut service = session_service(context)?;
    let snapshot = service.inspect_session(session_id)?;
    let thoughts = snapshot
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| {
            json!({
                "id": thought.id,
                "position": thought.position,
                "content": thought.content,
                "name": thought.name,
                "collapsed": thought.presentation.is_collapsed(),
                "presentation": thought.presentation.as_str(),
                "updated_at": thought.updated_at,
                "content_sha256": content_digest_hex(&thought.content),
            })
        })
        .collect::<Vec<_>>();
    let items = snapshot
        .board
        .live_items()
        .into_iter()
        .map(|item| match item {
            BoardItemRef::Thought(thought) => json!({
                "kind": "thought",
                "id": thought.id,
                "position": thought.position,
            }),
            BoardItemRef::Separator(separator) => json!({
                "kind": "separator",
                "id": separator.id,
                "position": separator.position,
                "created_at": separator.created_at,
                "updated_at": separator.updated_at,
            }),
        })
        .collect::<Vec<_>>();
    let human = snapshot
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| {
            let label = thought.name.as_ref().map_or_else(
                || excerpt(&thought.content),
                |name| name.as_str().to_owned(),
            );
            format!("{}  {}  {}", thought.position.get(), thought.id, label)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome {
        data: json!({ "session_id": session_id, "items": items, "thoughts": thoughts }),
        human,
    })
}

pub(super) fn inspect(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, true)?;
    drop(service);
    forwarding::sync(context, session_id)?;
    let mut service = session_service(context)?;
    let snapshot = service.inspect_session(session_id)?;
    let thought = snapshot.board.thought(thought_id).ok_or_else(|| {
        CliError::new(
            "thought_not_found",
            format!("thought not found: {thought_id}"),
            3,
        )
    })?;
    Ok(Outcome {
        data: json!({
            "session_id": session_id,
            "thought": {
                "id": thought.id,
                "content": thought.content,
                "name": thought.name,
                "position": thought.position,
                "collapsed": thought.presentation.is_collapsed(),
                "presentation": thought.presentation.as_str(),
                "deleted_at": thought.deleted_at,
                "content_sha256": content_digest_hex(&thought.content),
            }
        }),
        human: thought.content.clone(),
    })
}
