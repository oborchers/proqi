//! Target-aware built-in invocation choices shared by documented harnesses.

use crate::{
    application::{
        SHARED_PROMPT_STARTERS, supports_shared_starters as agent_supports_shared_starters,
    },
    ports::text_layout::byte_for_position,
    ui::app::BoardApp,
};

use super::{Choice, InvocationPopup, matcher};

pub(super) fn choices(app: &BoardApp, popup: &InvocationPopup) -> Vec<Choice> {
    if !starts_prompt(app, popup) || !app_supports_shared_starters(app) {
        return Vec::new();
    }
    SHARED_PROMPT_STARTERS
        .iter()
        .copied()
        .filter_map(|starter| {
            matcher::token(starter.token, &popup.query).map(|rank| (starter, rank))
        })
        .map(|(starter, rank)| Choice {
            token: starter.token.to_owned(),
            insertion: starter.token.to_owned(),
            annotation_display: None,
            separate_from_prefix: false,
            qualifier: "Shared Command".to_owned(),
            qualifier_fallbacks: Vec::new(),
            group: None,
            rank,
        })
        .collect()
}

pub(super) fn tokens(app: &BoardApp) -> impl Iterator<Item = &'static str> {
    let available = app_supports_shared_starters(app);
    SHARED_PROMPT_STARTERS
        .iter()
        .filter(move |_| available)
        .map(|starter| starter.token)
}

pub(super) fn is_shared_starter(token: &str) -> bool {
    SHARED_PROMPT_STARTERS
        .iter()
        .any(|starter| starter.token == token)
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

fn app_supports_shared_starters(app: &BoardApp) -> bool {
    app.agent_targets()
        .iter()
        .filter(|target| target.delivery.supports())
        .any(|target| agent_supports_shared_starters(target.agent_kind.as_str()))
}
