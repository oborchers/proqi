//! Board submission chord aliases at selection and insertion boundaries.

use super::{Fixture, draw, text};

use proqi::{
    application::Effect,
    domain::Direction,
    ports::agent::{AgentError, AgentState, SubmissionDisposition, SubmissionReceipt},
    ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};
use ratatui_core::layout::Rect;

#[test]
fn board_help_lists_both_spellings_while_the_footer_stays_compact() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let primary = if cfg!(target_os = "macos") {
        "Command+"
    } else {
        "Ctrl+"
    };
    let footer = text(draw(&mut fixture, 120, 12).backend().buffer());
    assert!(footer.contains("s Submit"));
    assert!(footer.contains("S Submit & keep"));
    assert!(!footer.contains(&format!("{primary}Enter/s")));

    fixture.input(UiInput::Key(UiKey::Character('?')));
    let help = text(draw(&mut fixture, 120, 32).backend().buffer());
    assert!(help.contains(&format!("{primary}Enter/s")));
    assert!(help.contains(&format!("{primary}Shift+Enter/S")));
    assert!(help.contains("Submit & keep"));
}

#[test]
fn primary_enter_submits_the_focused_thought_and_removes_only_after_acceptance() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let target = super::agent::target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

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
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
    fixture.input(UiInput::Key(UiKey::Redo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
}

#[test]
fn primary_shift_enter_keeps_one_contiguous_selection_after_one_ordered_delivery() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.paste("second thought with 👩‍💻 and e\u{301}");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.acknowledge_all_persistence();
    fixture.input(UiInput::Key(UiKey::Character('K')));
    let target = super::agent::target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::SubmitKeep));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(
        request.content,
        "exact prompt\nGrüße 第二行\n\nsecond thought with 👩‍💻 and e\u{301}"
    );
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Blocked),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn insertion_row_alias_matches_the_board_command_and_failure_keeps_the_source() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert!(fixture.app.insertion_focused());
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

    let effects = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    assert!(
        super::agent::finish_submission(&mut fixture, &request, Err(AgentError::TimedOut))
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(fixture.app.insertion_focused());
}

#[test]
fn empty_board_and_duplicate_alias_input_do_not_create_extra_attempts() {
    for key in [UiKey::Submit, UiKey::SubmitKeep] {
        let mut empty = Fixture::new();
        empty.input(UiInput::Key(UiKey::Escape));
        empty
            .app
            .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
        assert!(empty.effects(UiInput::Key(key)).is_empty());
        assert!(empty.app.state.board.live_thoughts().is_empty());
    }

    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let first = fixture.effects(UiInput::Key(UiKey::Submit));
    assert!(matches!(first.as_slice(), [Effect::PrepareSubmission(_)]));
    assert!(fixture.effects(UiInput::Key(UiKey::SubmitKeep)).is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("this thought already has a submission in progress")
    );
}

#[test]
fn primary_keep_alias_enters_the_existing_direction_and_mouse_path() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let up = super::agent::target(Direction::Up, "w1:p2");
    let right = super::agent::target(Direction::Right, "w1:p3");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![up.clone(), right]));

    assert!(fixture.effects(UiInput::Key(UiKey::SubmitKeep)).is_empty());
    assert_eq!(
        fixture.app.submission_mode(),
        Some(SubmissionDisposition::Keep)
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 10));
    let (_, control) = layout
        .controls
        .iter()
        .find(|(target, _)| {
            *target == proqi::ui::HitTarget::Deliver(Direction::Up, SubmissionDisposition::Keep)
        })
        .expect("mouse keep control");
    let clicked = fixture.effects(UiInput::Pointer(PointerInput {
        column: control.x,
        row: control.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let request = super::agent::start_submission(&mut fixture, &clicked);
    assert_eq!(request.target, up);
}
