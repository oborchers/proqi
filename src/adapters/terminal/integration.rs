//! Recognition-only integration metadata mapping.

pub(super) fn integration_context(
    target: &crate::ports::agent::AgentTarget,
    verified_at: crate::domain::Timestamp,
) -> Option<crate::domain::IntegrationContext> {
    let direction = target.adjacent_direction()?;
    Some(crate::domain::IntegrationContext {
        provider: "herdr".to_owned(),
        direction,
        agent_kind: target.agent_kind().as_str().to_owned(),
        agent_name: target.agent_name.clone(),
        workspace_hint: Some(target.workspace_id().to_owned()),
        tab_hint: Some(target.tab_id().to_owned()),
        pane_hint: Some(target.pane_id().to_owned()),
        verified_at,
    })
}
