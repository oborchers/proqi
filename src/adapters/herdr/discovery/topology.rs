//! Bounded, content-redacted topology labels for live invocation references.

use std::collections::BTreeMap;

use crate::ports::agent::AgentError;

use super::super::contract::{TabInfo, WorkspaceInfo};

pub(super) fn workspace_labels(
    values: Vec<WorkspaceInfo>,
    maximum: usize,
) -> Result<BTreeMap<String, Option<String>>, AgentError> {
    let mut labels = BTreeMap::new();
    for value in values.into_iter().take(maximum) {
        if labels
            .insert(value.workspace_id, bounded_topology_label(value.label))
            .is_some()
        {
            return Err(AgentError::Ambiguous(
                "duplicate workspace identity in Herdr snapshot".to_owned(),
            ));
        }
    }
    Ok(labels)
}

pub(super) fn tab_labels(
    values: Vec<TabInfo>,
    maximum: usize,
) -> Result<BTreeMap<String, (String, Option<String>)>, AgentError> {
    let mut labels = BTreeMap::new();
    for value in values.into_iter().take(maximum) {
        if labels
            .insert(
                value.tab_id,
                (value.workspace_id, bounded_topology_label(value.label)),
            )
            .is_some()
        {
            return Err(AgentError::Ambiguous(
                "duplicate tab identity in Herdr snapshot".to_owned(),
            ));
        }
    }
    Ok(labels)
}

pub(super) fn correlated_workspace_label(
    workspaces: &BTreeMap<String, Option<String>>,
    workspace_id: &str,
    truncated: bool,
) -> Result<Option<String>, AgentError> {
    if workspaces.is_empty() {
        return Ok(None);
    }
    match workspaces.get(workspace_id).cloned() {
        Some(label) => Ok(label),
        None if truncated => Ok(None),
        None => Err(AgentError::Malformed(
            "recognized agent has no matching workspace identity".to_owned(),
        )),
    }
}

pub(super) fn correlated_tab_label(
    tabs: &BTreeMap<String, (String, Option<String>)>,
    workspace_id: &str,
    tab_id: &str,
    truncated: bool,
) -> Result<Option<String>, AgentError> {
    if tabs.is_empty() {
        return Ok(None);
    }
    let Some((tab_workspace, label)) = tabs.get(tab_id) else {
        return if truncated {
            Ok(None)
        } else {
            Err(AgentError::Malformed(
                "recognized agent has no matching tab identity".to_owned(),
            ))
        };
    };
    if tab_workspace != workspace_id {
        return Err(AgentError::Malformed(
            "recognized agent and tab belong to different workspaces".to_owned(),
        ));
    }
    Ok(label.clone())
}

pub(super) fn sanitize_agent_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(32)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn bounded_topology_label(value: Option<String>) -> Option<String> {
    let value = value?;
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control())
        .take(48)
        .collect::<String>()
        .trim()
        .to_owned();
    (!sanitized.is_empty()).then_some(sanitized)
}
