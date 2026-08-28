//! Target-aware built-in invocation choices.

use crate::{
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
        text_layout::byte_for_position,
    },
    ui::app::BoardApp,
};

use super::{Choice, InvocationPopup};

const PLAN_TOKEN: &str = "/plan";

pub(super) fn plan_choice(app: &BoardApp, popup: &InvocationPopup) -> Option<Choice> {
    let harnesses = plan_harnesses(app)?;
    if !starts_prompt(app, popup) || !matches_query(popup) {
        return None;
    }
    Some(Choice {
        token: PLAN_TOKEN.to_owned(),
        label: format!("{PLAN_TOKEN}  Built-in Command · {harnesses}"),
    })
}

pub(super) fn starts_prompt(app: &BoardApp, popup: &InvocationPopup) -> bool {
    popup.range.as_ref().map_or_else(
        || {
            app.editor_snapshot().is_some_and(|snapshot| {
                snapshot.selection.is_none()
                    && byte_for_position(&snapshot.content, snapshot.cursor) == 0
            })
        },
        |range| range.start == 0,
    )
}

fn matches_query(popup: &InvocationPopup) -> bool {
    let query = popup.query.to_lowercase();
    if popup.manual {
        PLAN_TOKEN.contains(&query) || "plan".contains(&query)
    } else {
        PLAN_TOKEN.starts_with(&query)
    }
}

fn plan_harnesses(app: &BoardApp) -> Option<&'static str> {
    let mut codex = false;
    let mut claude = false;
    for target in app
        .agent_targets()
        .iter()
        .filter(|target| target.delivery.supports())
    {
        match target.agent_kind.as_str() {
            CODEX_AGENT_KIND => codex = true,
            CLAUDE_AGENT_KIND => claude = true,
            _ => {}
        }
    }
    match (codex, claude) {
        (true, true) => Some("Codex/Claude Code"),
        (true, false) => Some("Codex"),
        (false, true) => Some("Claude Code"),
        (false, false) => None,
    }
}
