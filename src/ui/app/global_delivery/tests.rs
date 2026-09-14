use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{Action, AppState, Effect},
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{
        agent::{
            AgentAvailability, AgentDeliveryCapabilities, AgentSessionBinding, AgentState,
            AgentTarget, HarnessKind, HerdrAgentAddress,
        },
        environment::IdGenerator,
    },
    ui::{BoardApp, UiKey, input::RoutedInput as UiInput},
};

#[test]
fn lower_delivery_owner_rejects_a_locked_source_if_reached() {
    let mut ids = FakeIdGenerator::new(1_725_220_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-global-delivery-lock"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought = Thought::new(
        ids.thought_id(),
        session.id,
        "locked source".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let thought_id = thought.id;
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    assert!(
        app.reduce(Action::BeginSubmission {
            thought_ids: vec![thought_id]
        })
        .is_empty()
    );

    let effects = app.begin_global_delivery(&mut ids, &clock);
    let [Effect::DiscoverGlobalAgents { generation }] = effects.as_slice() else {
        panic!("expected global discovery");
    };
    app.complete_global_agent_discovery(*generation, Ok(vec![target()]));
    assert!(
        app.handle_global_delivery_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock)
            .is_empty()
    );
    assert!(
        app.handle_global_delivery_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock)
            .is_empty()
    );
    assert_eq!(
        app.status_text(),
        Some("a selected thought already has a submission in progress")
    );
    assert!(
        app.state
            .board
            .thought(thought_id)
            .is_some_and(Thought::is_live)
    );
}

fn target() -> AgentTarget {
    AgentTarget::herdr_agent(
        20,
        HerdrAgentAddress::new(
            "w2".to_owned(),
            "w2:t1".to_owned(),
            "w2:p8".to_owned(),
            HarnessKind::new("codex").expect("harness"),
            AgentSessionBinding::established("session-w2:p8").expect("session"),
        )
        .expect("address"),
        "Receiver".to_owned(),
        Some("Workspace w2".to_owned()),
        Some("Tab w2:t1".to_owned()),
        AgentState::Idle,
        AgentAvailability::Available,
        AgentDeliveryCapabilities::SUBMIT_ONLY,
    )
}
