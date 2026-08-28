//! Target-aware built-in invocation choices shared by documented harnesses.

use crate::{
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
        text_layout::byte_for_position,
    },
    ui::app::BoardApp,
};

use super::{Choice, InvocationPopup};

#[derive(Clone, Copy)]
struct SharedStarter {
    token: &'static str,
    search_name: &'static str,
}

const SHARED_STARTERS: [SharedStarter; 2] = [
    SharedStarter {
        token: "/goal",
        search_name: "goal",
    },
    SharedStarter {
        token: "/plan",
        search_name: "plan",
    },
];

pub(super) fn choices(app: &BoardApp, popup: &InvocationPopup) -> Vec<Choice> {
    if !starts_prompt(app, popup) || !supports_shared_starters(app) {
        return Vec::new();
    }
    SHARED_STARTERS
        .iter()
        .copied()
        .filter(|starter| matches_query(*starter, popup))
        .map(|starter| Choice {
            token: starter.token.to_owned(),
            label: format!("{}  Shared Command", starter.token),
        })
        .collect()
}

pub(super) fn tokens(app: &BoardApp) -> impl Iterator<Item = &'static str> {
    let available = supports_shared_starters(app);
    SHARED_STARTERS
        .iter()
        .filter(move |_| available)
        .map(|starter| starter.token)
}

pub(in crate::ui::app) fn without_later_shared_starter(content: &str) -> &str {
    let Some(starter) = SHARED_STARTERS
        .iter()
        .find(|starter| content.starts_with(starter.token))
    else {
        return content;
    };
    let Some(remainder) = content.get(starter.token.len()..) else {
        return content;
    };
    let Some(separator) = remainder.chars().next() else {
        return remainder;
    };
    if !separator.is_whitespace() {
        return content;
    }
    let separator_len = if remainder.starts_with("\r\n") {
        2
    } else {
        separator.len_utf8()
    };
    &remainder[separator_len..]
}

pub(super) fn is_shared_starter(token: &str) -> bool {
    SHARED_STARTERS.iter().any(|starter| starter.token == token)
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

fn matches_query(starter: SharedStarter, popup: &InvocationPopup) -> bool {
    let query = popup.query.to_lowercase();
    if popup.manual {
        starter.token.contains(&query) || starter.search_name.contains(&query)
    } else {
        starter.token.starts_with(&query)
    }
}

fn supports_shared_starters(app: &BoardApp) -> bool {
    app.agent_targets()
        .iter()
        .filter(|target| target.delivery.supports())
        .any(|target| {
            matches!(
                target.agent_kind.as_str(),
                CODEX_AGENT_KIND | CLAUDE_AGENT_KIND
            )
        })
}
