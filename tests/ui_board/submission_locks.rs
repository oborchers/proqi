use super::*;

use proqi::domain::Direction;

#[test]
fn every_board_mutation_stays_locked_until_submission_is_journaled() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = super::agent::start_submission(&mut fixture, &effects);

    for input in [
        UiInput::Key(UiKey::Character('d')),
        UiInput::Key(UiKey::PrimaryCharacter('J')),
        UiInput::Key(UiKey::Undo),
    ] {
        assert!(fixture.effects(input).is_empty());
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
        assert!(
            fixture
                .app
                .status_text()
                .is_some_and(|status| { status.contains("submission in progress") })
        );
    }

    assert!(matches!(
        fixture
            .app
            .complete_submission(
                request.submission_id,
                Err(proqi::ports::agent::AgentError::TimedOut),
            )
            .as_slice(),
        [Effect::FinishSubmission { .. }]
    ));
    assert!(matches!(
        fixture
            .app
            .complete_submission_journaled(request.submission_id, Ok(()))
            .as_slice(),
        []
    ));
    assert!(matches!(
        fixture
            .effects(UiInput::Key(UiKey::Character('d')))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
}

#[test]
fn failed_submission_preparation_releases_the_application_lock() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let [Effect::PrepareSubmission(attempt)] = effects.as_slice() else {
        panic!("expected one submission preparation, got {effects:?}");
    };
    assert!(
        fixture
            .app
            .complete_submission_prepared(
                attempt.id,
                Err(proqi::ports::store::StoreError::DiskFull),
            )
            .is_empty()
    );
    assert!(matches!(
        fixture
            .effects(UiInput::Key(UiKey::Character('d')))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
}
