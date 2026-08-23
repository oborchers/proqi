//! Recognition-only integration metadata mapping.

pub(super) fn integration_context(
    target: &crate::ports::agent::AgentTarget,
    verified_at: crate::domain::Timestamp,
) -> crate::domain::IntegrationContext {
    crate::domain::IntegrationContext {
        provider: "herdr".to_owned(),
        direction: target.direction,
        agent_kind: target.agent_kind.clone(),
        agent_name: target.agent_name.clone(),
        workspace_hint: Some(target.workspace_id.clone()),
        tab_hint: Some(target.tab_id.clone()),
        pane_hint: Some(target.pane_id.clone()),
        verified_at,
    }
}
