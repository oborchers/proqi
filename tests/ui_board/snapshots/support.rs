//! Shared target fixtures for snapshots that exercise agent-aware board chrome.

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentSessionBinding, AgentState, AgentTarget, CODEX_AGENT_KIND, HarnessKind, PaneContext,
        PaneRect,
    },
};

pub(super) fn adjacent_target(
    direction: Direction,
    pane_id: &str,
    readiness: AgentState,
) -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 40,
            y: 20,
            width: 40,
            height: 20,
        },
    };
    AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 19,
        direction,
        pane_id: pane_id.to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        agent_name: format!("Codex {pane_id}"),
        agent_session: AgentSessionBinding::established(format!("session-{pane_id}"))
            .expect("fixture session"),
        readiness,
        delivery: proqi::ports::agent::AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: source.rect,
        source,
    }
}
