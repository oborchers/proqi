use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentState, SubmissionReceipt},
};

#[test]
fn accepted_first_prompt_upgrades_the_cached_target_session() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional = super::agent::target(Direction::Left, "w1:p2");
    provisional.agent_session_id = None;
    fixture
        .app
        .complete_agent_discovery(Ok(vec![provisional.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    let mut established = provisional;
    established.agent_session_id = Some("new-codex-session".to_owned());
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
fn a_receipt_before_the_session_hook_is_accepted_and_refreshes_identity() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional = super::agent::target(Direction::Left, "w1:p2");
    provisional.agent_session_id = None;
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
        ] if target.agent_session_id.is_none()
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.app.agent_targets(), [provisional.clone()]);
    assert_eq!(
        fixture.app.status_text(),
        Some("submitted left to Codex w1:p2, thought kept")
    );

    let mut established = provisional;
    established.agent_session_id = Some("new-codex-session".to_owned());
    fixture
        .app
        .complete_agent_discovery(Ok(vec![established.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let second = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(second.target, established);
}
