//! Ranked session projections for JSON, human output, and the Session Browser.

use std::{
    collections::{HashMap, HashSet},
    str::FromStr as _,
};

use serde_json::json;

use crate::{
    domain::SessionId,
    ports::{runtime::RuntimeCoordinator, store::SessionHit},
    ui::{BrowserAvailability, SessionBrowserItem},
};

use super::{
    super::{
        super::args::PageArgs,
        pagination::{human_page, paginate},
    },
    CliError, Outcome, RuntimeContext, session_service,
};

pub(in crate::cli::execute) fn list_sessions(
    context: &mut RuntimeContext,
    query: Option<String>,
    all: bool,
    page: &PageArgs,
) -> Result<Outcome, CliError> {
    let runtime = context.coordinator.scan_runtime()?;
    let active: HashSet<_> = runtime
        .active
        .into_iter()
        .map(|instance| instance.session_id)
        .collect();
    let recovered: HashSet<_> = runtime.recovered.into_iter().collect();
    let hits = session_service(context)?.list_sessions(query, all)?;
    let page = paginate(
        hits,
        page,
        |hit| hit.id,
        |value| {
            SessionId::from_str(value).map_err(|error| {
                CliError::identifier(format!("invalid session identifier {value}: {error}"))
            })
        },
    )?;
    let data: Vec<_> = page
        .entries
        .iter()
        .map(|hit| {
            json!({
                "id": hit.id,
                "name": hit.name,
                "origin_cwd": hit.origin_cwd,
                "last_opened_cwd": hit.last_opened_cwd,
                "last_opened_at": hit.last_opened_at,
                "last_active_at": hit.last_active_at,
                "thought_count": hit.thought_count,
                "excerpt": hit.excerpt,
                "previews": hit.previews,
                "integration_context": hit.integration_context,
                "state": session_state(hit, &active, &recovered),
            })
        })
        .collect();
    let human = human_page(
        &page,
        human_lines(&page.entries, &active, &recovered),
        "No sessions",
    );
    Ok(Outcome {
        data: json!({
            "sessions": data,
            "total": page.total,
            "next_after": page.next_after,
        }),
        human,
    })
}

pub(super) fn browser_items(
    context: &mut RuntimeContext,
) -> Result<Vec<SessionBrowserItem>, CliError> {
    let runtime = context.coordinator.scan_runtime()?;
    let mut active: HashMap<_, _> = runtime
        .active
        .into_iter()
        .map(|instance| (instance.session_id, instance))
        .collect();
    let recovered: HashSet<_> = runtime.recovered.into_iter().collect();
    let hits = session_service(context)?.list_sessions(None, true)?;
    Ok(hits
        .into_iter()
        .map(|hit| {
            let availability = if hit.trashed {
                BrowserAvailability::Trashed
            } else if let Some(instance) = active.remove(&hit.id) {
                BrowserAvailability::Active(instance)
            } else if recovered.contains(&hit.id) {
                BrowserAvailability::Recovered
            } else {
                BrowserAvailability::Resumable
            };
            SessionBrowserItem { hit, availability }
        })
        .collect())
}

fn human_lines(
    hits: &[SessionHit],
    active: &HashSet<SessionId>,
    recovered: &HashSet<SessionId>,
) -> Vec<String> {
    hits.iter()
        .map(|hit| {
            let state = session_state(hit, active, recovered);
            let label = hit.name.as_deref().unwrap_or(&hit.excerpt);
            format!(
                "{}  {state}  {}  {label}",
                hit.id,
                hit.last_opened_cwd.display()
            )
        })
        .collect()
}

pub(in crate::cli::execute) fn session_state(
    hit: &SessionHit,
    active: &HashSet<SessionId>,
    recovered: &HashSet<SessionId>,
) -> &'static str {
    availability_state(hit.id, hit.trashed, active, recovered)
}

/// Stable availability spelling shared by every session projection.
pub(super) fn availability_state(
    id: SessionId,
    trashed: bool,
    active: &HashSet<SessionId>,
    recovered: &HashSet<SessionId>,
) -> &'static str {
    if trashed {
        "trashed"
    } else if active.contains(&id) {
        "active"
    } else if recovered.contains(&id) {
        "recovered"
    } else {
        "resumable"
    }
}
