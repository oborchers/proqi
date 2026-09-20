//! Target-aware built-in invocation choices shared by documented harnesses.

use crate::{
    application::{
        SHARED_HARNESS_COMMANDS, supports_shared_commands as agent_supports_shared_commands,
    },
    ports::text_layout::byte_for_position,
    ui::app::BoardApp,
};

use super::{Choice, InvocationPopup, matcher};

pub(super) fn choices(app: &BoardApp, popup: &InvocationPopup) -> Vec<Choice> {
    if !starts_prompt(app, popup) || !app_supports_shared_commands(app) {
        return Vec::new();
    }
    SHARED_HARNESS_COMMANDS
        .iter()
        .copied()
        .filter_map(|command| {
            matcher::token(command.token, popup.query.text()).map(|rank| (command, rank))
        })
        .map(|(command, rank)| Choice {
            token: command.token.to_owned(),
            insertion: command.token.to_owned(),
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
    let available = app_supports_shared_commands(app);
    SHARED_HARNESS_COMMANDS
        .iter()
        .filter(move |_| available)
        .map(|command| command.token)
}

pub(super) fn is_shared_command(token: &str) -> bool {
    SHARED_HARNESS_COMMANDS
        .iter()
        .any(|command| command.token == token)
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

fn app_supports_shared_commands(app: &BoardApp) -> bool {
    app.agent_targets()
        .iter()
        .filter(|target| target.delivery.supports())
        .any(|target| agent_supports_shared_commands(target.agent_kind().as_str()))
}
