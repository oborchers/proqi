//! Read-only thought projections for human and JSON callers.

use crate::cli::error_code::ErrorCode;
use serde_json::json;

use crate::domain::BoardItemRef;

use super::{
    super::args::PageArgs,
    CliError, Outcome, forwarding,
    helpers::{content_digest_hex, excerpt, parse_item_id, parse_thought_id},
    pagination::paginate,
    session_service,
};
use crate::cli::runtime::RuntimeContext;

pub(super) fn list(
    context: &mut RuntimeContext,
    reference: &str,
    page: &PageArgs,
) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(reference, true)?;
    drop(service);
    forwarding::sync(context, session_id)?;
    let mut service = session_service(context)?;
    let snapshot = service.inspect_session(session_id)?;
    let page = paginate(
        snapshot.board.live_items(),
        page,
        |item| item.id(),
        parse_item_id,
    )?;
    let items = page.entries.iter().map(item_json).collect::<Vec<_>>();
    let human = page
        .entries
        .iter()
        .filter_map(|item| match item {
            BoardItemRef::Thought(thought) => {
                let label = thought.name.as_ref().map_or_else(
                    || excerpt(&thought.content),
                    |name| name.as_str().to_owned(),
                );
                Some(format!(
                    "{}  {}  {}",
                    thought.position.get(),
                    thought.id,
                    label
                ))
            }
            BoardItemRef::Separator(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome {
        data: json!({
            "session_id": session_id,
            "items": items,
            "total": page.total,
            "next_after": page.next_after,
        }),
        human,
    })
}

fn item_json(item: &BoardItemRef<'_>) -> serde_json::Value {
    match item {
        BoardItemRef::Thought(thought) => json!({
            "kind": "thought",
            "id": thought.id,
            "position": thought.position,
            "content": thought.content,
            "name": thought.name,
            "collapsed": thought.presentation.is_collapsed(),
            "presentation": thought.presentation.as_str(),
            "updated_at": thought.updated_at,
            "content_sha256": content_digest_hex(&thought.content),
        }),
        BoardItemRef::Separator(separator) => json!({
            "kind": "separator",
            "id": separator.id,
            "position": separator.position,
            "created_at": separator.created_at,
            "updated_at": separator.updated_at,
        }),
    }
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
            ErrorCode::ThoughtNotFound,
            format!("thought not found: {thought_id}"),
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
