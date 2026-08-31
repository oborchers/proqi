use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentError, AgentState, SubmissionReceipt},
};

#[test]
fn direct_edit_chords_submit_only_the_active_thought_and_preserve_mode_on_failure_or_keep() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    let thought_id = fixture.app.active_thought_id().expect("active thought");
    let target = super::agent::target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let failed = fixture.effects(UiInput::Key(UiKey::Submit));
    let failed_request = super::agent::start_submission(&mut fixture, &failed);
    assert_eq!(failed_request.content, "exact prompt\nGrüße 第二行");
    assert!(
        super::agent::finish_submission(&mut fixture, &failed_request, Err(AgentError::TimedOut),)
            .is_empty()
    );
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { thought_id }
    );

    let keeping = fixture.effects(UiInput::Key(UiKey::SubmitKeep));
    let request = super::agent::start_submission(&mut fixture, &keeping);
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
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { thought_id }
    );

    let removing = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &removing);
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: request.target.clone(),
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
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Compose
    );
    assert!(fixture.app.compose_prompt_visible());

    let next = fixture.effects(UiInput::Key(UiKey::Character('n')));
    assert!(matches!(next.as_slice(), [Effect::CommitBoardOperation(_)]));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "n");
}

#[test]
fn direct_edit_submission_without_a_verified_target_keeps_the_complete_draft() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    let before = fixture.app.editor_snapshot().expect("editor");

    let effects = fixture.effects(UiInput::Key(UiKey::Submit));

    assert!(matches!(effects.as_slice(), [Effect::DiscoverAgents]));
    assert_eq!(fixture.app.editor_snapshot(), Some(before));
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn direct_submission_controls_follow_editor_controls_in_the_footer() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

    let layout = fixture
        .app
        .prepare_frame(ratatui_core::layout::Rect::new(0, 0, 120, 12));
    let editor_row = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == proqi::ui::HitTarget::Copy).then_some(area.y))
        .expect("editor controls");
    let submission_row = layout
        .controls
        .iter()
        .find_map(|(target, area)| {
            matches!(target, proqi::ui::HitTarget::Deliver(_, _)).then_some(area.y)
        })
        .expect("submission controls");
    assert!(submission_row > editor_row);

    let terminal = draw(&mut fixture, 120, 12);
    let rendered = text(terminal.backend().buffer());
    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    assert!(rendered.contains(&format!("{primary}Enter Submit")));
    assert!(rendered.contains(&format!("{primary}Shift+Enter Submit & keep")));
}
