use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentError, AgentState, SubmissionReceipt},
};

fn stage_removal_while_unrelated_edit_is_pending(
    fixture: &mut Fixture,
) -> (proqi::domain::ThoughtId, proqi::domain::OperationSequence) {
    let first_sequence = fixture.paste("submitted");
    let source = fixture.app.active_thought_id().expect("source");
    fixture.app.acknowledge_persistence(first_sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));
    let second_sequence = fixture.paste("unrelated");
    let unrelated = fixture.app.active_thought_id().expect("unrelated");
    fixture.app.acknowledge_persistence(second_sequence, true);
    fixture.input(UiInput::Key(UiKey::Character('x')));
    proqi::application::reduce(
        &mut fixture.app.state,
        proqi::application::Action::BeginSubmission {
            thought_ids: vec![source],
        },
    )
    .expect("lock source");
    let stage = proqi::application::reduce(
        &mut fixture.app.state,
        proqi::application::Action::StageSubmissionRemoval {
            operation_id: fixture.ids.operation_id(),
            thought_ids: vec![source],
            at: Timestamp::from_millis(30),
        },
    )
    .expect("stage removal");
    let [Effect::CommitBoardOperation(removal)] = stage.as_slice() else {
        panic!("expected removal operation");
    };
    (unrelated, removal.sequence)
}

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
}

#[test]
fn direct_submit_removal_waits_for_durability_and_retries_without_losing_edit_state() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    let thought_id = fixture.app.active_thought_id().expect("active thought");
    let target = super::agent::target(Direction::Right, "w1:p2");
    fixture.app.complete_agent_discovery(Ok(vec![target]));
    let before = fixture.app.editor_snapshot().expect("editor");

    let removing = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &removing);
    let journal = fixture.app.complete_submission(
        request.submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: request.target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );
    let [
        Effect::FinishSubmission {
            removal: Some(operation),
            ..
        },
    ] = journal.as_slice()
    else {
        panic!("accepted removal must be journaled with its operation");
    };
    let removal_sequence = operation.sequence;
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { thought_id }
    );
    assert_eq!(fixture.app.editor_snapshot(), Some(before));

    fixture.app.acknowledge_persistence(removal_sequence, false);
    fixture.app.submission_persistence_failed(
        request.submission_id,
        &proqi::ports::store::StoreError::DiskFull,
    );
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { thought_id }
    );
    assert!(fixture.app.state.thought_locked(thought_id));
    assert!(matches!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))).as_slice(),
        [Effect::RetryPersistence { sequence }] if *sequence == removal_sequence
    ));

    fixture.app.acknowledge_persistence(removal_sequence, true);
    let completion = fixture
        .app
        .complete_submission_journaled(request.submission_id, Ok(()));
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
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
fn accepted_removal_freezes_content_until_its_durable_acknowledgement() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let before = fixture.app.editor_snapshot().expect("editor");
    let clipboard = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = clipboard.as_slice() else {
        panic!("expected clipboard read");
    };
    let clipboard_request = *request_id;
    let removing = fixture.effects(UiInput::Key(UiKey::Submit));
    let request = super::agent::start_submission(&mut fixture, &removing);
    let journal = fixture.app.complete_submission(
        request.submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: request.target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(matches!(
        journal.as_slice(),
        [Effect::FinishSubmission {
            removal: Some(_),
            ..
        }]
    ));

    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('!')))
            .is_empty()
    );
    assert!(
        fixture
            .app
            .complete_clipboard_read(
                clipboard_request,
                Ok("stale paste".to_owned()),
                &mut fixture.ids,
                &fixture.clock,
            )
            .is_empty()
    );
    assert_eq!(fixture.app.editor_snapshot(), Some(before));
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
fn escape_reaches_board_while_direct_submission_keeps_its_source_locked() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Enter));
    let thought_id = fixture.app.active_thought_id().expect("active thought");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));
    let effects = fixture.effects(UiInput::Key(UiKey::Submit));
    let _request = super::agent::start_submission(&mut fixture, &effects);

    assert!(fixture.effects(UiInput::Key(UiKey::Escape)).is_empty());
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
    assert!(fixture.app.state.thought_locked(thought_id));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
}

#[test]
fn staged_removal_cannot_discard_an_unrelated_pending_editor_revision() {
    let mut fixture = Fixture::new();
    let (unrelated, removal_sequence) = stage_removal_while_unrelated_edit_is_pending(&mut fixture);

    assert!(fixture.effects(UiInput::Key(UiKey::Escape)).is_empty());
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit {
            thought_id: unrelated
        }
    );
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("retained editor")
            .content,
        "unrelatedx"
    );

    fixture.app.acknowledge_persistence(removal_sequence, true);
    let revision = fixture.effects(UiInput::Key(UiKey::Escape));
    let [Effect::CommitRevision(revision)] = revision.as_slice() else {
        panic!("expected pending revision after removal receipt");
    };
    fixture.app.acknowledge_persistence(revision.sequence, true);
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("re-entered editor")
            .content,
        "unrelatedx"
    );
}

#[test]
fn staged_removal_blocks_stale_submit_undo_redo_and_quit_boundaries() {
    for key in [
        UiKey::Submit,
        UiKey::SubmitKeep,
        UiKey::Undo,
        UiKey::Redo,
        UiKey::Quit,
    ] {
        let mut fixture = Fixture::new();
        let (unrelated, _removal_sequence) =
            stage_removal_while_unrelated_edit_is_pending(&mut fixture);
        fixture
            .app
            .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

        assert!(fixture.effects(UiInput::Key(key)).is_empty());
        assert!(!fixture.app.quit);
        assert!(!fixture.app.state.thought_locked(unrelated));
        assert_eq!(
            fixture
                .app
                .editor_snapshot()
                .expect("retained editor")
                .content,
            "unrelatedx"
        );
        assert_eq!(
            fixture.app.interaction_mode(),
            proqi::application::InteractionMode::Edit {
                thought_id: unrelated
            }
        );
    }
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
        "Cmd+"
    } else {
        "Ctrl+"
    };
    assert!(rendered.contains(&format!("{primary}Enter Submit")));
    assert!(rendered.contains(&format!("{primary}Shift+Enter Submit & keep")));
}
