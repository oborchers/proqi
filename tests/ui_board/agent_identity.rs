use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentState, SubmissionReceipt},
};

#[test]
fn accepted_receipt_ignores_volatile_target_metadata() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let target = super::agent::target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    let mut revalidated = target;
    revalidated.readiness = AgentState::Blocked;
    revalidated.agent_name = "Renamed agent".to_owned();
    let mut rect = revalidated
        .route
        .adjacent_target_rect()
        .expect("adjacent rect");
    rect.x = rect.x.saturating_add(1);
    let mut source = revalidated
        .route
        .adjacent_source()
        .expect("adjacent source")
        .clone();
    source.rect.width = source.rect.width.saturating_add(1);
    revalidated = revalidated.with_adjacent_geometry(rect, source);

    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: revalidated,
            post_state: Some(AgentState::Unknown),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}
