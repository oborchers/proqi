use super::*;

use proqi::ports::agent::{
    AgentAvailability, AgentDeliveryCapabilities, AgentSessionBinding, AgentState, AgentTarget,
    HarnessKind, HerdrAgentAddress, SubmissionReceipt, SubmissionRouteKind,
};

pub(super) fn target(
    workspace: &str,
    tab: &str,
    pane: &str,
    name: &str,
    readiness: AgentState,
    availability: AgentAvailability,
) -> AgentTarget {
    AgentTarget::herdr_agent(
        20,
        HerdrAgentAddress::new(
            workspace.to_owned(),
            tab.to_owned(),
            pane.to_owned(),
            HarnessKind::new("codex").expect("harness"),
            AgentSessionBinding::established(format!("session-{pane}")).expect("session"),
        )
        .expect("address"),
        name.to_owned(),
        Some(format!("Workspace {workspace}")),
        Some(format!("Tab {tab}")),
        readiness,
        availability,
        AgentDeliveryCapabilities::SUBMIT_ONLY,
    )
}

pub(super) fn open(fixture: &mut Fixture) -> u64 {
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "submit to agent".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    assert_eq!(
        fixture.app.palette_view().expect("commands").1,
        ["Submit to agent..."]
    );
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    let [Effect::DiscoverGlobalAgents { generation }] = effects.as_slice() else {
        panic!("expected current-server discovery: {effects:?}");
    };
    *generation
}

pub(super) fn prepare(fixture: &mut Fixture, content: &str) {
    let sequence = fixture.paste(content);
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));
}

#[test]
fn commands_only_target_search_requires_an_explicit_disposition() {
    let mut fixture = Fixture::new();
    prepare(&mut fixture, "focused Grüße\n\u{1b}[31m");
    let normal = text(draw(&mut fixture, 80, 8).backend().buffer());
    assert!(!normal.contains("Submit to agent"));

    let generation = open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![
            target(
                "w1",
                "w1:t2",
                "w1:p2",
                "Alpha",
                AgentState::Idle,
                AgentAvailability::Available,
            ),
            target(
                "w2",
                "w2:t1",
                "w2:p8",
                "Béta 世界",
                AgentState::Working,
                AgentAvailability::Available,
            ),
        ]),
    );
    for character in "béta".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let chooser = text(draw(&mut fixture, 72, 9).backend().buffer());
    assert!(chooser.contains("Béta"), "{chooser}");
    assert!(chooser.contains("世 界"), "{chooser}");
    assert!(!chooser.contains("Alpha"));

    assert!(fixture.effects(UiInput::Key(UiKey::Enter)).is_empty());
    let disposition = text(draw(&mut fixture, 72, 8).backend().buffer());
    assert!(disposition.contains("Submit"));
    assert!(disposition.contains("Submit and keep"));
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    let [Effect::PrepareSubmission(attempt)] = effects.as_slice() else {
        panic!("expected durable reservation: {effects:?}");
    };
    assert_eq!(attempt.route.kind(), SubmissionRouteKind::HerdrAgent);
    assert_eq!(attempt.route.adjacent_direction(), None);
    assert_eq!(attempt.route.version(), 1);
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "focused Grüße\n\u{1b}[31m");
    assert_eq!(request.target.pane_id(), "w2:p8");
}

#[test]
fn keep_and_remove_dispositions_share_receipt_matching_and_removal_contracts() {
    for keep in [false, true] {
        let mut fixture = Fixture::new();
        prepare(&mut fixture, "one");
        let destination = target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Receiver",
            AgentState::Done,
            AgentAvailability::Available,
        );
        let generation = open(&mut fixture);
        fixture
            .app
            .complete_global_agent_discovery(generation, Ok(vec![destination.clone()]));
        fixture.input(UiInput::Key(UiKey::Enter));
        if keep {
            fixture.input(UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            }));
        }
        let effects = fixture.effects(UiInput::Key(UiKey::Enter));
        let request = super::agent::start_submission(&mut fixture, &effects);
        let completion = super::agent::finish_submission(
            &mut fixture,
            &request,
            Ok(SubmissionReceipt {
                submission_id: request.submission_id,
                target: destination,
                post_state: Some(AgentState::Working),
            }),
        );
        assert!(
            completion.is_empty(),
            "global delivery stores no adjacent context"
        );
        assert_eq!(
            fixture.app.state.board.live_thoughts().len(),
            usize::from(keep)
        );
        if !keep {
            fixture.input(UiInput::Key(UiKey::Escape));
            fixture.input(UiInput::Key(UiKey::Undo));
            assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
        }
    }
}

#[test]
fn global_provisional_receipt_does_not_refresh_adjacent_targets() {
    let mut fixture = Fixture::new();
    prepare(&mut fixture, "sessionless global");
    let destination = target(
        "w2",
        "w2:t1",
        "w2:p8",
        "Sessionless",
        AgentState::Idle,
        AgentAvailability::Available,
    )
    .with_agent_session(AgentSessionBinding::provisional());
    let generation = open(&mut fixture);
    fixture
        .app
        .complete_global_agent_discovery(generation, Ok(vec![destination.clone()]));
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    let prepared = fixture.effects(UiInput::Key(UiKey::Enter));
    let request = super::agent::start_submission(&mut fixture, &prepared);
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: destination,
            post_state: Some(AgentState::Idle),
        }),
    );

    assert!(completion.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn blocked_unknown_launching_and_noninteractive_targets_stay_visible_but_disabled() {
    for (availability, expected) in [
        (AgentAvailability::Blocked, "agent is blocked"),
        (AgentAvailability::Unknown, "agent state is unknown"),
        (AgentAvailability::Launching, "agent is still launching"),
        (
            AgentAvailability::NotInteractive,
            "agent is not interactive yet",
        ),
    ] {
        let mut fixture = Fixture::new();
        prepare(&mut fixture, "keep me");
        let generation = open(&mut fixture);
        fixture.app.complete_global_agent_discovery(
            generation,
            Ok(vec![target(
                "w1",
                "w1:t2",
                "w1:p2",
                "Unavailable",
                AgentState::Unknown,
                availability,
            )]),
        );
        let rendered = text(draw(&mut fixture, 70, 7).backend().buffer());
        assert!(rendered.contains(availability.as_str()));
        assert!(fixture.effects(UiInput::Key(UiKey::Enter)).is_empty());
        assert!(
            fixture
                .app
                .status_text()
                .is_some_and(|status| status.starts_with(expected))
        );
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    }
}

#[test]
fn stale_completion_is_ignored_and_reopening_refreshes_new_targets() {
    let mut fixture = Fixture::new();
    prepare(&mut fixture, "source");
    let stale = open(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Escape));
    let current = open(&mut fixture);
    assert_ne!(stale, current);
    fixture.app.complete_global_agent_discovery(
        stale,
        Ok(vec![target(
            "w1",
            "w1:t2",
            "w1:p2",
            "Stale",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    fixture.app.complete_global_agent_discovery(
        current,
        Ok(vec![target(
            "w2",
            "w2:t1",
            "w2:p8",
            "New target",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    let rendered = text(draw(&mut fixture, 60, 7).backend().buffer());
    assert!(rendered.contains("New target"));
    assert!(!rendered.contains("Stale"));
}

#[test]
fn mouse_activation_and_shallow_resize_use_the_same_semantic_rows() {
    let mut fixture = Fixture::new();
    prepare(&mut fixture, "mouse source");
    let generation = open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Mäuse 世界",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    for (width, height) in [(22, 5), (80, 9), (30, 4)] {
        let _terminal = draw(&mut fixture, width, height);
    }
    let target_area = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 64, 7))
        .overlay
        .expect("target overlay")
        .items[0];
    let target_effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: target_area.x,
        row: target_area.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(target_effects.is_empty());
    fixture.clock.set(Timestamp::from_millis(1_000));
    let behavior_area = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 64, 7))
        .overlay
        .expect("behavior overlay")
        .items[1];
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: behavior_area.x,
        row: behavior_area.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let [Effect::PrepareSubmission(attempt)] = effects.as_slice() else {
        panic!("expected mouse submission reservation: {effects:?}");
    };
    assert_eq!(
        attempt.disposition,
        proqi::ports::agent::SubmissionDisposition::Keep
    );
}

#[test]
fn discontiguous_selection_keeps_exact_board_order_for_global_delivery() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        prepare(&mut fixture, content);
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let generation = open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Receiver",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.content, "first\n\nthird");
}

#[test]
fn invocation_reference_annotations_remain_inert_exact_prompt_content() {
    let mut fixture = Fixture::new();
    let content = "Ask @receiver to inspect this";
    let start = content.find("@receiver").expect("reference start");
    let end = start + "@receiver".len();
    let payload = PastePayload::annotated(
        content.to_owned(),
        vec![ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::InvocationReference {
                display_name: "@receiver · codex".to_owned(),
            },
        }],
    )
    .expect("annotated prompt");
    let effects = fixture.effects(UiInput::PasteAnnotated(payload));
    let sequence = effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("persistence sequence");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));

    let generation = open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Different receiver",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    fixture.input(UiInput::Key(UiKey::Enter));
    let prepared = fixture.effects(UiInput::Key(UiKey::Enter));
    let request = super::agent::start_submission(&mut fixture, &prepared);
    assert_eq!(request.content, content);
    assert_eq!(request.target.pane_id(), "w2:p8");
}

#[test]
fn source_change_during_discovery_aborts_before_journal_or_delivery() {
    let mut fixture = Fixture::new();
    prepare(&mut fixture, "original");
    let thought = fixture.app.state.board.live_thoughts()[0].clone();
    let generation = open(&mut fixture);
    let effects = proqi::application::reduce(
        &mut fixture.app.state,
        proqi::application::Action::EditThought {
            thought_id: thought.id,
            revision_id: fixture.ids.revision_id(),
            before_content: thought.content,
            after_content: "changed externally".to_owned(),
            before_annotations: thought.annotations,
            after_annotations: Vec::new(),
            before_cursor: proqi::domain::TextPosition::default(),
            after_cursor: proqi::domain::TextPosition::default(),
            at: Timestamp::from_millis(30),
        },
    )
    .expect("external edit");
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Receiver",
            AgentState::Idle,
            AgentAvailability::Available,
        )]),
    );
    fixture.input(UiInput::Key(UiKey::Enter));
    let submission = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(submission.is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("source changed during agent selection; thoughts kept")
    );
}
