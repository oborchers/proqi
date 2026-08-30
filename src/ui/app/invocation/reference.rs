//! Typed live collaborator choices within the canonical invocation picker.

use std::ops::Range;

use crate::{application::Effect, ports::invocation::LiveAgentReference};

use super::{BoardApp, Choice, InvocationPopup};

const MAX_LIVE_RESULTS: usize = 10;

impl BoardApp {
    pub(in crate::ui::app) fn refresh_invocation_popup_after_input(
        &mut self,
        mut effects: Vec<Effect>,
    ) -> Vec<Effect> {
        let was_open = self.invocation_popup.is_some();
        self.refresh_invocation_popup();
        if !was_open && self.invocation_popup.is_some() {
            effects.extend(self.refresh_invocations());
        }
        effects
    }
}

pub(super) fn choices(
    app: &BoardApp,
    popup: &InvocationPopup,
    normalized_query: &str,
) -> Vec<Choice> {
    app.invocation_live
        .iter()
        .filter(|reference| matches(reference, popup.manual, normalized_query))
        .take(MAX_LIVE_RESULTS)
        .map(choice)
        .collect()
}

fn choice(reference: &LiveAgentReference) -> Choice {
    Choice {
        token: reference.agent_name().to_owned(),
        insertion: insertion(reference),
        separate_from_prefix: true,
        qualifier: format!(
            "{} {} {}",
            reference.harness(),
            reference.pane_id(),
            reference.state().as_str()
        ),
        group: Some(group(reference)),
    }
}

fn group(reference: &LiveAgentReference) -> String {
    let identities = format!("{}/{}", reference.workspace_id(), reference.tab_id());
    let labels = match (reference.workspace_label(), reference.tab_label()) {
        (None, None) => String::new(),
        (workspace, tab) => format!(
            " · {}/{}",
            workspace.unwrap_or(reference.workspace_id()),
            tab.unwrap_or(reference.tab_id())
        ),
    };
    format!("{} · {identities}{labels}", reference.provider().label())
}

fn insertion(reference: &LiveAgentReference) -> String {
    let workspace = labeled_identity(reference.workspace_label(), reference.workspace_id());
    let tab = labeled_identity(reference.tab_label(), reference.tab_id());
    format!(
        "Herdr collaborator: {} ({}) at workspace {}, tab {}, pane {}",
        reference.agent_name(),
        reference.harness(),
        workspace,
        tab,
        reference.pane_id()
    )
}

pub(super) fn insertion_text(choice: &Choice, content: &str, range: &Range<usize>) -> String {
    let prefix = if choice.separate_from_prefix
        && range.is_empty()
        && content[..range.start]
            .chars()
            .next_back()
            .is_some_and(|character| !character.is_whitespace())
    {
        " "
    } else {
        ""
    };
    format!("{prefix}{} ", choice.insertion)
}

fn labeled_identity(label: Option<&str>, identity: &str) -> String {
    label.map_or_else(
        || identity.to_owned(),
        |label| format!("{label} ({identity})"),
    )
}

fn matches(reference: &LiveAgentReference, manual: bool, normalized_query: &str) -> bool {
    if manual {
        return [
            reference.agent_name(),
            reference.harness().as_str(),
            reference.workspace_id(),
            reference.tab_id(),
            reference.pane_id(),
            reference.provider().label(),
        ]
        .iter()
        .any(|value| value.to_lowercase().contains(normalized_query))
            || reference
                .workspace_label()
                .is_some_and(|label| label.to_lowercase().contains(normalized_query))
            || reference
                .tab_label()
                .is_some_and(|label| label.to_lowercase().contains(normalized_query));
    }
    let Some(name_query) = normalized_query.strip_prefix('@') else {
        return false;
    };
    !name_query.is_empty()
        && (reference
            .agent_name()
            .to_lowercase()
            .starts_with(name_query)
            || reference.pane_id().to_lowercase().starts_with(name_query))
}
