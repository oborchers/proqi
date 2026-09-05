//! Semantic rows shared by global delivery rendering, selection, and hit geometry.

use crate::ports::agent::{AgentAvailability, AgentTarget, SubmissionDisposition};

use super::{
    GlobalChoice, GlobalDeliveryChoiceView, GlobalDeliveryStage, GlobalDeliveryState,
    GlobalDeliveryView,
};

impl GlobalDeliveryState {
    pub(super) const fn no_choice_message(&self) -> &'static str {
        match &self.stage {
            GlobalDeliveryStage::Targets { loading: true, .. } => {
                "agent discovery is still in progress"
            }
            GlobalDeliveryStage::Targets {
                failure: Some(_), ..
            } => "agent discovery failed; refresh and try again",
            GlobalDeliveryStage::Targets { .. } => "no compatible agent matches this search",
            GlobalDeliveryStage::Disposition { .. } => "choose a submission behavior",
        }
    }

    pub(super) fn match_count(&self) -> usize {
        match &self.stage {
            GlobalDeliveryStage::Targets { loading: true, .. } => 1,
            GlobalDeliveryStage::Targets { .. } => self.matching_targets().len().max(1),
            GlobalDeliveryStage::Disposition { .. } => 2,
        }
    }

    pub(super) fn clamp(&mut self) {
        self.selected = self.selected.min(self.match_count().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
    }

    pub(super) fn choice(&self) -> Option<GlobalChoice> {
        match &self.stage {
            GlobalDeliveryStage::Targets { loading: true, .. } => None,
            GlobalDeliveryStage::Targets { .. } => self
                .matching_targets()
                .get(self.selected)
                .map(|target| GlobalChoice::Target((*target).clone())),
            GlobalDeliveryStage::Disposition { target } => {
                let disposition = match self.selected {
                    0 => SubmissionDisposition::RemoveAfterSuccess,
                    1 => SubmissionDisposition::Keep,
                    _ => return None,
                };
                Some(GlobalChoice::Disposition(
                    disposition,
                    target.as_ref().clone(),
                    self.thought_ids.clone(),
                    self.source_digests.clone(),
                ))
            }
        }
    }

    pub(super) fn view(&self) -> GlobalDeliveryView {
        let (title, choices) = match &self.stage {
            GlobalDeliveryStage::Targets { loading: true, .. } => (
                " submit to agent ",
                vec![placeholder("Discovering current-server agents...")],
            ),
            GlobalDeliveryStage::Targets {
                failure: Some(code),
                ..
            } => (
                " submit to agent ",
                vec![placeholder(&format!(
                    "Discovery unavailable ({})",
                    code.as_str()
                ))],
            ),
            GlobalDeliveryStage::Targets { .. } => {
                let matches = self.matching_targets();
                let rows = if matches.is_empty() {
                    vec![placeholder("No compatible agents match")]
                } else {
                    matches
                        .into_iter()
                        .skip(self.scroll)
                        .map(target_view)
                        .collect()
                };
                (" submit to agent ", rows)
            }
            GlobalDeliveryStage::Disposition { .. } => (
                " submission behavior ",
                vec![
                    GlobalDeliveryChoiceView {
                        primary: "Submit".to_owned(),
                        secondary: "remove after accepted receipt".to_owned(),
                        secondary_fallbacks: Vec::new(),
                        protected_secondaries: Vec::new(),
                        enabled: true,
                    },
                    GlobalDeliveryChoiceView {
                        primary: "Submit and keep".to_owned(),
                        secondary: "keep after accepted receipt".to_owned(),
                        secondary_fallbacks: Vec::new(),
                        protected_secondaries: Vec::new(),
                        enabled: true,
                    },
                ],
            ),
        };
        GlobalDeliveryView {
            title,
            query: self.query.text().to_owned(),
            choices,
            selected: self.selected.saturating_sub(self.scroll),
        }
    }

    fn matching_targets(&self) -> Vec<&AgentTarget> {
        let GlobalDeliveryStage::Targets { targets, .. } = &self.stage else {
            return Vec::new();
        };
        let query = self.query.text().to_lowercase();
        targets
            .iter()
            .filter(|target| query.is_empty() || searchable_target(target).contains(&query))
            .collect()
    }
}

fn searchable_target(target: &AgentTarget) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        target.agent_name,
        target.workspace_label.as_deref().unwrap_or_default(),
        target.tab_label.as_deref().unwrap_or_default(),
        target.workspace_id(),
        target.tab_id(),
        target.pane_id(),
        target.agent_kind().as_str(),
        target.readiness.as_str(),
    )
    .to_lowercase()
}

fn target_view(target: &AgentTarget) -> GlobalDeliveryChoiceView {
    let workspace = target
        .workspace_label
        .as_deref()
        .unwrap_or_else(|| target.workspace_id());
    let tab = target
        .tab_label
        .as_deref()
        .unwrap_or_else(|| target.tab_id());
    let pane = compact_child(target.workspace_id(), target.pane_id());
    let state = target_live_state(target);
    let location_and_pane = format!("{workspace} / {tab} · {pane}");
    let location_without_harness = format!("{location_and_pane} · {state}");
    let workspace_and_pane = format!("{workspace} · {pane} · {state}");
    let pane_and_state = format!("{pane} · {state}");
    GlobalDeliveryChoiceView {
        primary: target.agent_name.clone(),
        secondary: format!(
            "{location_and_pane} · {} · {state}",
            target.agent_kind().as_str(),
        ),
        secondary_fallbacks: vec![location_without_harness, workspace_and_pane],
        protected_secondaries: vec![pane_and_state, state.to_owned()],
        enabled: target.can_submit(),
    }
}

fn compact_child<'a>(workspace: &str, identity: &'a str) -> &'a str {
    identity
        .strip_prefix(workspace)
        .and_then(|suffix| suffix.strip_prefix(':'))
        .unwrap_or(identity)
}

const fn target_live_state(target: &AgentTarget) -> &'static str {
    match target.availability {
        AgentAvailability::Available => target.readiness.as_str(),
        availability => availability.as_str(),
    }
}

fn placeholder(value: &str) -> GlobalDeliveryChoiceView {
    GlobalDeliveryChoiceView {
        primary: value.to_owned(),
        secondary: String::new(),
        secondary_fallbacks: Vec::new(),
        protected_secondaries: Vec::new(),
        enabled: false,
    }
}
