//! Typed live collaborator choices within the canonical invocation picker.

use std::ops::Range;

use crate::{
    application::Effect,
    ports::invocation::{
        InvocationReferenceDiscovery, InvocationReferenceDiscoveryRequest, LiveAgentReference,
    },
    ui::PastePayload,
};

use super::{BoardApp, Choice, InvocationPopup};

impl BoardApp {
    pub(in crate::ui::app) fn open_invocation_picker(&mut self) -> Vec<Effect> {
        if !matches!(
            self.interaction_mode(),
            crate::application::InteractionMode::Edit { .. }
        ) {
            self.enter_edit();
        }
        if !matches!(
            self.interaction_mode(),
            crate::application::InteractionMode::Edit { .. }
        ) {
            self.set_warning("focus a thought before inserting an invocation");
            return Vec::new();
        }
        self.invocation_popup = Some(InvocationPopup {
            query: String::new(),
            range: None,
            manual: true,
            selected: 0,
            scroll: 0,
        });
        self.begin_reference_refresh()
    }

    pub(in crate::ui::app) fn close_invocation_picker(&mut self) {
        self.invocation_popup = None;
        self.invocation_live.clear();
        self.invocation_completeness.clear_provider();
        if self.invocation_reference_pending.take().is_some() {
            self.invocation_reference_generation =
                self.invocation_reference_generation.wrapping_add(1);
        }
    }

    pub(in crate::ui::app) fn refresh_invocation_popup_after_input(
        &mut self,
        mut effects: Vec<Effect>,
    ) -> Vec<Effect> {
        let was_open = self.invocation_popup.is_some();
        self.refresh_invocation_popup();
        let opened = !was_open && self.invocation_popup.is_some();
        let awaiting_automatic_reference = self.invocation_popup.is_none()
            && self.invocation_reference_pending.is_none()
            && self.reference_token_is_active();
        if opened || awaiting_automatic_reference {
            effects.extend(self.begin_reference_refresh());
        }
        effects
    }

    /// Apply only the newest live-reference result to the currently active picker generation.
    pub fn complete_invocation_reference_discovery(
        &mut self,
        discovery: InvocationReferenceDiscovery,
    ) -> bool {
        if self.invocation_reference_pending != Some(discovery.generation)
            || discovery.generation != self.invocation_reference_generation
        {
            return false;
        }
        self.invocation_reference_pending = None;
        self.invocation_live = discovery.references;
        self.invocation_completeness
            .set_provider(discovery.completeness);
        if self.invocation_popup.is_some() {
            self.clamp_invocation_popup();
        } else if matches!(
            self.interaction_mode(),
            crate::application::InteractionMode::Edit { .. }
        ) && self.reference_token_is_active()
        {
            self.refresh_invocation_popup();
        }
        true
    }

    fn begin_reference_refresh(&mut self) -> Vec<Effect> {
        self.invocation_reference_generation = self.invocation_reference_generation.wrapping_add(1);
        let generation = self.invocation_reference_generation;
        self.invocation_reference_pending = Some(generation);
        self.invocation_live.clear();
        self.invocation_completeness.clear_provider();
        vec![Effect::DiscoverInvocationReferences(
            InvocationReferenceDiscoveryRequest { generation },
        )]
    }

    fn reference_token_is_active(&self) -> bool {
        self.active_invocation_token()
            .is_some_and(|(query, _)| query.starts_with('@'))
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
        .map(|reference| choice(reference, &app.invocation_live))
        .collect()
}

fn choice(reference: &LiveAgentReference, all: &[LiveAgentReference]) -> Choice {
    let token = primary_label(reference).to_owned();
    let (qualifier, qualifier_fallbacks) = qualifiers(reference, &token);
    Choice {
        insertion: insertion(reference, &token),
        annotation_display: Some(mention_label(reference, all)),
        separate_from_prefix: true,
        qualifier,
        qualifier_fallbacks,
        group: Some(reference.provider().label().to_owned()),
        token,
    }
}

fn primary_label(reference: &LiveAgentReference) -> &str {
    reference
        .agent_name()
        .or_else(|| reference.tab_label().filter(|label| meaningful_tab(label)))
        .unwrap_or_else(|| reference.harness().as_str())
}

fn qualifiers(reference: &LiveAgentReference, primary: &str) -> (String, Vec<String>) {
    let location = compact_location(reference, primary);
    let pane = compact_child(reference.workspace_id(), reference.pane_id());
    let location_and_pane = format!("{location} · {pane}");
    let harness = (!same_display(reference.harness().as_str(), primary))
        .then_some(reference.harness().as_str());
    let without_state = harness.map_or_else(
        || location_and_pane.clone(),
        |harness| format!("{location_and_pane} · {harness}"),
    );
    let full = format!("{without_state} · {}", reference.state().as_str());
    let mut fallbacks = vec![without_state];
    if fallbacks[0] != location_and_pane {
        fallbacks.push(location_and_pane.clone());
    }
    let workspace = reference
        .workspace_label()
        .unwrap_or_else(|| reference.workspace_id());
    let workspace_and_pane = format!("{workspace} · {pane}");
    if workspace_and_pane != location_and_pane {
        fallbacks.push(workspace_and_pane);
    }
    fallbacks.push(pane.to_owned());
    (full, fallbacks)
}

fn compact_location(reference: &LiveAgentReference, primary: &str) -> String {
    let workspace = reference
        .workspace_label()
        .unwrap_or_else(|| reference.workspace_id());
    let tab = reference
        .tab_label()
        .filter(|label| meaningful_tab(label))
        .filter(|label| !same_display(label, primary))
        .map(ToOwned::to_owned)
        .or_else(|| {
            reference
                .tab_label()
                .is_none()
                .then(|| compact_child(reference.workspace_id(), reference.tab_id()).to_owned())
        });
    tab.map_or_else(
        || workspace.to_owned(),
        |tab| format!("{workspace} / {tab}"),
    )
}

fn meaningful_tab(label: &str) -> bool {
    !label.chars().all(|character| character.is_ascii_digit())
}

fn compact_child<'a>(workspace: &str, identity: &'a str) -> &'a str {
    identity
        .strip_prefix(workspace)
        .and_then(|suffix| suffix.strip_prefix(':'))
        .unwrap_or(identity)
}

fn insertion(reference: &LiveAgentReference, primary: &str) -> String {
    let workspace = labeled_identity(reference.workspace_label(), reference.workspace_id());
    let tab = labeled_identity(reference.tab_label(), reference.tab_id());
    format!(
        "Herdr collaborator: {primary} ({}) at workspace {workspace}, tab {tab}, pane {}",
        reference.harness(),
        reference.pane_id()
    )
}

fn mention_label(reference: &LiveAgentReference, all: &[LiveAgentReference]) -> String {
    let base = mention_base(reference);
    if all
        .iter()
        .filter(|candidate| same_display(&mention_base(candidate), &base))
        .count()
        == 1
    {
        return base;
    }
    let human = mention_with_location(&base, reference, false);
    if all
        .iter()
        .filter(|candidate| {
            same_display(
                &mention_with_location(&mention_base(candidate), candidate, false),
                &human,
            )
        })
        .count()
        == 1
    {
        return human;
    }
    mention_with_location(&base, reference, true)
}

fn mention_base(reference: &LiveAgentReference) -> String {
    let primary = primary_label(reference);
    let subject = primary.trim_start_matches('@');
    let subject = if subject.is_empty() {
        reference.harness().as_str()
    } else {
        subject
    };
    if same_display(primary, reference.harness().as_str()) {
        format!("@{subject}")
    } else {
        format!("@{subject} · {}", reference.harness())
    }
}

fn mention_with_location(base: &str, reference: &LiveAgentReference, exact: bool) -> String {
    if exact {
        return format!("{base} · pane {}", reference.pane_id());
    }
    let workspace = reference
        .workspace_label()
        .unwrap_or_else(|| reference.workspace_id());
    let pane = compact_child(reference.workspace_id(), reference.pane_id());
    format!("{base} · {workspace}/{pane}")
}

fn same_display(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

pub(super) fn insertion_payload(
    choice: &Choice,
    content: &str,
    range: &Range<usize>,
) -> PastePayload {
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
    let start = prefix.len();
    let end = start.saturating_add(choice.insertion.len());
    let content = format!("{prefix}{} ", choice.insertion);
    match &choice.annotation_display {
        Some(display_name) => {
            PastePayload::invocation_reference(content, start..end, display_name.clone())
        }
        None => PastePayload::text(content),
    }
}

fn labeled_identity(label: Option<&str>, identity: &str) -> String {
    label.map_or_else(
        || identity.to_owned(),
        |label| format!("{label} ({identity})"),
    )
}

fn matches(reference: &LiveAgentReference, manual: bool, normalized_query: &str) -> bool {
    let primary = primary_label(reference);
    if manual {
        return reference
            .agent_name()
            .is_some_and(|name| contains(name, normalized_query))
            || [
                primary,
                reference.harness().as_str(),
                reference.workspace_id(),
                reference.tab_id(),
                reference.pane_id(),
                reference.provider().label(),
            ]
            .iter()
            .any(|value| contains(value, normalized_query))
            || reference
                .workspace_label()
                .is_some_and(|label| contains(label, normalized_query))
            || reference
                .tab_label()
                .is_some_and(|label| contains(label, normalized_query));
    }
    let Some(name_query) = normalized_query.strip_prefix('@') else {
        return false;
    };
    !name_query.is_empty()
        && (primary.to_lowercase().starts_with(name_query)
            || reference.pane_id().to_lowercase().starts_with(name_query))
}

fn contains(value: &str, query: &str) -> bool {
    value.to_lowercase().contains(query)
}
