use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{AgentError, AgentGateway, AgentState, SubmissionRequest},
        environment::IdGenerator,
    },
};

use super::{
    capability_responses, discovery_responses, gateway, neighbor, right_rect, source, success,
};

fn recorded_agents(name: &str) -> Value {
    let fixture = match name {
        "idle" => include_str!("../../../../tests/fixtures/herdr/hermes/agent-list.idle.json"),
        "working" => {
            include_str!("../../../../tests/fixtures/herdr/hermes/agent-list.working.json")
        }
        "launch-pending" => {
            include_str!("../../../../tests/fixtures/herdr/hermes/agent-list.launch-pending.json")
        }
        "replaced" => {
            include_str!("../../../../tests/fixtures/herdr/hermes/agent-list.replaced.json")
        }
        "exited" => {
            include_str!("../../../../tests/fixtures/herdr/hermes/agent-list.exited.json")
        }
        _ => panic!("unknown recorded Hermes fixture"),
    };
    let document: Value = serde_json::from_str(fixture).expect("recorded Hermes JSON");
    document["result"]["agents"].clone()
}

fn prompted() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/herdr/hermes/agent-prompted.accepted.json"
    ))
    .expect("recorded Hermes prompt receipt")
}

fn discover(name: &str) -> (super::HerdrGateway<super::FakeRunner>, super::FakeRunner) {
    let context = source();
    gateway(discovery_responses(
        &context,
        recorded_agents(name),
        Some(("w1:p2", right_rect())),
    ))
}

#[test]
fn recorded_hermes_identity_uses_the_open_established_session_path() {
    for (fixture, readiness) in [("idle", AgentState::Idle), ("working", AgentState::Working)] {
        let context = source();
        let (mut gateway, _) = discover(fixture);
        let targets = gateway
            .adjacent_targets(&context)
            .expect("established Hermes target");
        let [target] = targets.as_slice() else {
            panic!("expected one recorded Hermes target");
        };
        assert_eq!(target.agent_kind().as_str(), "hermes");
        assert_eq!(target.agent_name, "hermes-qualifier");
        assert_eq!(
            target.agent_session().as_id(),
            Some("hermes-session-fixture-a")
        );
        assert_eq!(target.readiness, readiness);
    }
}

#[test]
fn recorded_hermes_target_survives_unrelated_adjacent_shells() {
    let context = source();
    let up = crate::ports::agent::PaneRect {
        x: 15,
        y: 0,
        width: 10,
        height: 10,
    };
    let down = crate::ports::agent::PaneRect {
        x: 15,
        y: 30,
        width: 10,
        height: 10,
    };
    let left = crate::ports::agent::PaneRect {
        x: 0,
        y: 15,
        width: 10,
        height: 10,
    };
    let mut responses = capability_responses(&context);
    responses.push(success(serde_json::json!({
        "result":{"agents":recorded_agents("idle")}
    })));
    responses.push(success(neighbor(
        &context,
        crate::domain::Direction::Up,
        Some(("w1:p2", up)),
    )));
    responses.push(success(neighbor(
        &context,
        crate::domain::Direction::Right,
        Some(("w1:p3", right_rect())),
    )));
    responses.push(success(neighbor(
        &context,
        crate::domain::Direction::Down,
        Some(("w1:p4", down)),
    )));
    responses.push(success(neighbor(
        &context,
        crate::domain::Direction::Left,
        Some(("w1:p5", left)),
    )));
    let (mut gateway, _) = gateway(responses);

    let targets = gateway
        .adjacent_targets(&context)
        .expect("ordinary shell neighbors are ignored");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].agent_kind().as_str(), "hermes");
}

#[test]
fn recorded_hermes_sessionless_unready_and_exited_states_fail_closed() {
    let context = source();
    let (mut pending, _) = discover("launch-pending");
    assert!(
        pending
            .adjacent_targets(&context)
            .expect("launch-pending Hermes is hidden")
            .is_empty()
    );
    let (mut exited, _) = discover("exited");
    assert!(
        exited
            .adjacent_targets(&context)
            .expect("exited Hermes is absent")
            .is_empty()
    );

    let mut sessionless = recorded_agents("idle");
    sessionless[0]
        .as_object_mut()
        .expect("recorded Hermes agent")
        .remove("agent_session");
    let (mut gateway, _) = gateway(discovery_responses(
        &context,
        sessionless,
        Some(("w1:p2", right_rect())),
    ));
    assert!(
        gateway
            .adjacent_targets(&context)
            .expect("sessionless Hermes is hidden")
            .is_empty()
    );
}

#[test]
fn recorded_hermes_receipt_preserves_identity_and_exact_prompt_data() {
    let context = source();
    let (mut discovery, _) = discover("idle");
    let target = discovery
        .adjacent_targets(&context)
        .expect("Hermes discovery")
        .pop()
        .expect("Hermes target");
    let mut responses = discovery_responses(
        &context,
        recorded_agents("idle"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(prompted()));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let prompt_text = "  quotes: ‘\"'\tGrüße e\u{301} 🧑‍💻\n$(touch never); * ? [x]  \n";
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target,
            content: prompt_text.to_owned(),
        })
        .expect("accepted established Hermes prompt");

    assert_eq!(receipt.target.agent_kind().as_str(), "hermes");
    assert_eq!(
        receipt.target.agent_session().as_id(),
        Some("hermes-session-fixture-a")
    );
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    let prompt = requests.last().expect("semantic prompt request");
    assert_eq!(
        prompt.args,
        ["agent", "prompt", "w1:p2", prompt_text],
        "prompt remains one exact argument"
    );
    assert_eq!(prompt.stdin, None);
}

#[test]
fn recorded_hermes_replacement_and_receipt_session_change_fail_closed() {
    let context = source();
    let (mut discovery, _) = discover("idle");
    let target = discovery
        .adjacent_targets(&context)
        .expect("Hermes discovery")
        .pop()
        .expect("Hermes target");

    let (mut replaced, runner) = gateway(discovery_responses(
        &context,
        recorded_agents("replaced"),
        Some(("w1:p2", right_rect())),
    ));
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    assert!(matches!(
        replaced.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: target.clone(),
            content: "must not send".to_owned(),
        }),
        Err(AgentError::Unsupported(_))
    ));
    assert!(
        !runner
            .requests
            .borrow()
            .iter()
            .any(|request| { request.args.get(1).is_some_and(|value| value == "prompt") })
    );

    let mut changed_receipt = prompted();
    changed_receipt["result"]["agent"]["agent_session"]["value"] =
        Value::String("hermes-session-fixture-b".to_owned());
    let mut responses = discovery_responses(
        &context,
        recorded_agents("idle"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(changed_receipt));
    let (mut changed, _) = gateway(responses);
    assert!(matches!(
        changed.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target,
            content: "keep on mismatch".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}
