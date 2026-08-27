use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentSessionBinding, AgentState, OPENCODE_AGENT_KIND, SubmissionReceipt},
};

#[test]
fn accepted_first_opencode_prompt_upgrades_the_cached_target_session() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional =
        super::agent::target_with_kind(Direction::Left, "w1:p2", OPENCODE_AGENT_KIND);
    provisional.agent_session = AgentSessionBinding::provisional();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![provisional.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    let mut established = provisional;
    established.agent_session =
        AgentSessionBinding::established("new-codex-session").expect("fixture session");
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: established.clone(),
            post_state: Some(AgentState::Working),
        }),
    );

    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert_eq!(fixture.app.agent_targets(), [established.clone()]);

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let second = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(second.target, established);
}

#[test]
fn an_opencode_receipt_before_the_session_hook_refreshes_without_resending() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional =
        super::agent::target_with_kind(Direction::Left, "w1:p2", OPENCODE_AGENT_KIND);
    provisional.agent_session = AgentSessionBinding::provisional();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![provisional.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let request = super::agent::start_submission(&mut fixture, &effects);

    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: provisional.clone(),
            post_state: Some(AgentState::Working),
        }),
    );

    assert!(matches!(
        completion.as_slice(),
        [
            Effect::StoreIntegrationContext { target, .. },
            Effect::DiscoverAgents
        ] if target.agent_session.is_provisional()
    ));
    assert!(
        !completion
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitAgent(_)))
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.app.agent_targets(), [provisional.clone()]);
    assert_eq!(
        fixture.app.status_text(),
        Some("submitted left to opencode w1:p2, thought kept")
    );

    let mut established = provisional;
    established.agent_session =
        AgentSessionBinding::established("new-codex-session").expect("fixture session");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![established.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let second = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(second.target, established);
}
