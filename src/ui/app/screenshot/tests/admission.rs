use super::behavior::{app_with_thought, candidate, created, next_commit};
use crate::{
    application::{DurabilityState, Effect},
    domain::{Direction, OperationSequence, Timestamp},
    ports::{
        agent::{
            AgentDeliveryCapabilities, AgentSessionBinding, AgentState, AgentTarget,
            CODEX_AGENT_KIND, HarnessKind, PaneContext, PaneRect, SubmissionDisposition,
            SubmissionReceipt,
        },
        environment::IdGenerator as _,
        store::{CommitReceipt, DurableIdentity, SessionHit},
    },
    ui::{UiInput, UiKey},
};

#[test]
fn cut_and_capture_use_distinct_sequences_in_both_completion_orderings() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    let cut_request = app.handle(UiInput::Key(UiKey::Cut), &mut ids, &clock);
    let [Effect::WriteClipboard { request_id, .. }] = cut_request.as_slice() else {
        panic!("pending cut");
    };
    app.queue_screenshot_candidates([candidate(60)]);
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    let cut_effects = app.complete_clipboard_write(*request_id, Ok(()), &mut ids, &clock);
    let [Effect::CommitBoardOperation(cut)] = cut_effects.as_slice() else {
        panic!("durable cut");
    };
    app.acknowledge_persistence(cut.sequence, true);
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(capture.operation.sequence > cut.sequence);

    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(61)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(
        app.handle(UiInput::Key(UiKey::Cut), &mut ids, &clock)
            .is_empty()
    );
    let replay = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    let [Effect::WriteClipboard { request_id, .. }] = replay.as_slice() else {
        panic!("replayed cut");
    };
    let cut_effects = app.complete_clipboard_write(*request_id, Ok(()), &mut ids, &clock);
    let [Effect::CommitBoardOperation(cut)] = cut_effects.as_slice() else {
        panic!("cut after capture");
    };
    assert!(cut.sequence > capture.operation.sequence);
    assert!(matches!(
        app.state.durability,
        DurabilityState::Pending { .. }
    ));
}

#[test]
fn submit_remove_and_capture_use_distinct_sequences_in_both_orderings() {
    let (mut app, mut ids, clock, thought_id) = app_with_thought();
    let target = agent_target();
    let submission = app.queue_submission(
        &target,
        SubmissionDisposition::RemoveAfterSuccess,
        &[thought_id],
        &mut ids,
        &clock,
    );
    let [Effect::PrepareSubmission(attempt)] = submission.as_slice() else {
        panic!("submission intent");
    };
    let submission_id = attempt.id;
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(62)]);
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    let submit_sequence = finish_submission(&mut app, &target, submission_id);
    app.acknowledge_persistence(submit_sequence, true);
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(capture.operation.sequence > submit_sequence);

    let (mut app, mut ids, clock, _) = app_with_thought();
    let target = agent_target();
    app.complete_agent_discovery(Ok(vec![target.clone()]));
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(63)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Character('s')), &mut ids, &clock);
    let replay = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    let [Effect::PrepareSubmission(attempt)] = replay.as_slice() else {
        panic!("replayed submission");
    };
    let submit_sequence = finish_submission(&mut app, &target, attempt.id);
    assert!(submit_sequence > capture.operation.sequence);
}

#[test]
fn transfer_remove_and_capture_use_distinct_sequences_in_both_orderings() {
    let (mut app, mut ids, clock, thought_id) = app_with_thought();
    let destination = ids.session_id();
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(Ok(vec![session_hit(destination)]));
    let transfer_effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThought(request)] = transfer_effects.as_slice() else {
        panic!("transfer intent");
    };
    let request = request.clone();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(64)]);
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    let transfer_sequence = finish_transfer(&mut app, &mut ids, &clock, &request, destination);
    app.acknowledge_persistence(transfer_sequence, true);
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(capture.operation.sequence > transfer_sequence);
    assert_ne!(thought_id, capture_thought_id(&capture));

    let (mut app, mut ids, clock, _) = app_with_thought();
    let destination = ids.session_id();
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(Ok(vec![session_hit(destination)]));
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(65)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let replay = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    let [Effect::TransferThought(request)] = replay.as_slice() else {
        panic!("replayed transfer");
    };
    let transfer_sequence = finish_transfer(&mut app, &mut ids, &clock, request, destination);
    assert!(transfer_sequence > capture.operation.sequence);
}

fn finish_submission(
    app: &mut crate::ui::BoardApp,
    target: &AgentTarget,
    submission_id: crate::domain::SubmissionId,
) -> OperationSequence {
    app.complete_submission_prepared(submission_id, Ok(()));
    let delivery = app.complete_submission_sending(submission_id, Ok(()));
    let [Effect::SubmitAgent(request)] = delivery.as_slice() else {
        panic!("submission delivery");
    };
    app.complete_submission(
        submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );
    app.complete_submission_journaled(submission_id, Ok(()))
        .iter()
        .find_map(|effect| match effect {
            Effect::CommitBoardOperation(operation) => Some(operation.sequence),
            _ => None,
        })
        .unwrap_or_else(|| panic!("submission removal"))
}

fn finish_transfer(
    app: &mut crate::ui::BoardApp,
    ids: &mut crate::adapters::memory::FakeIdGenerator,
    clock: &crate::adapters::memory::FakeClock,
    request: &crate::ports::transfer::SessionTransferRequest,
    destination: crate::domain::SessionId,
) -> OperationSequence {
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };
    let completion = app.complete_session_transfer(
        request,
        Ok(crate::application::ThoughtMutation {
            thought_id: ids.thought_id(),
            receipt,
        }),
        ids,
        clock,
    );
    let [Effect::CommitBoardOperation(operation)] = completion.as_slice() else {
        panic!("transfer removal");
    };
    operation.sequence
}

fn capture_thought_id(capture: &crate::ports::store::CaptureCommit) -> crate::domain::ThoughtId {
    match &capture.operation.forward {
        crate::domain::BoardMutation::AddThought { thought } => thought.id,
        other => panic!("unexpected capture mutation: {other:?}"),
    }
}

fn agent_target() -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
    };
    AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 19,
        direction: Direction::Right,
        pane_id: "w1:p2".to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: HarnessKind::new(CODEX_AGENT_KIND).expect("agent kind"),
        agent_name: "codex".to_owned(),
        agent_session: AgentSessionBinding::established("session").expect("agent session"),
        readiness: AgentState::Idle,
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: PaneRect {
            x: 20,
            y: 0,
            width: 20,
            height: 20,
        },
        source,
    }
}

fn session_hit(id: crate::domain::SessionId) -> SessionHit {
    SessionHit {
        id,
        name: Some("destination".to_owned()),
        origin_cwd: std::env::temp_dir(),
        last_opened_cwd: std::env::temp_dir(),
        last_opened_at: Timestamp::from_millis(1),
        last_active_at: Timestamp::from_millis(1),
        thought_count: 0,
        excerpt: String::new(),
        previews: Vec::new(),
        search_content: String::new(),
        integration_context: None,
        trashed: false,
    }
}
