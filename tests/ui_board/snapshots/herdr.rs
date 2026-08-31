use proqi::{
    application::Effect,
    domain::Direction,
    ports::{
        agent::{
            AgentDeliveryCapabilities, AgentSessionBinding, AgentState, AgentTarget,
            CODEX_AGENT_KIND, HarnessKind, PaneContext, PaneRect,
        },
        invocation::{
            InvocationReferenceDiscovery, InvocationReferenceProvider, LiveAgentReference,
        },
    },
};

use super::Fixture;

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
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: source.rect,
        source,
    }
}

pub(super) fn complete_live_reference(fixture: &mut Fixture, effects: &[Effect]) {
    let [Effect::DiscoverInvocationReferences(request)] = effects else {
        panic!("live reference refresh effect");
    };
    let reference = LiveAgentReference::new(
        InvocationReferenceProvider::Herdr,
        Some("reviewer".to_owned()),
        HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        "w2".to_owned(),
        Some("Product".to_owned()),
        "w2:t4".to_owned(),
        Some("Review".to_owned()),
        "w2:p9".to_owned(),
        AgentState::Working,
    )
    .expect("live reference");
    fixture
        .app
        .complete_invocation_reference_discovery(InvocationReferenceDiscovery {
            generation: request.generation,
            references: Ok(vec![reference]),
        });
}
