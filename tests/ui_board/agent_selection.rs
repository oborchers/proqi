use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentState, SubmissionReceipt},
};

#[test]
fn selected_thoughts_submit_once_in_board_order_and_remove_as_one_undo_step() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.paste("second thought");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let target = super::agent::target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(
        request.content,
        "exact prompt\nGrüße 第二行\n\nsecond thought"
    );
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [
            Effect::StoreIntegrationContext { .. },
            Effect::CommitBoardOperation(_)
        ]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}
