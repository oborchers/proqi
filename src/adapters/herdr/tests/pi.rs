use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{AgentError, AgentGateway, AgentSessionBinding, HarnessKind, SubmissionRequest},
        environment::IdGenerator,
    },
};

use super::{discovery_responses, gateway, right_rect, source, success, target};

fn fixture(name: &str) -> Value {
    let raw = match name {
        "established" => include_str!("../fixtures/pi/agent_list.established.json"),
        "launch_pending" => include_str!("../fixtures/pi/agent_list.launch_pending.json"),
        "working" => include_str!("../fixtures/pi/agent_list.working.json"),
        "prompted" => include_str!("../fixtures/pi/agent_prompted.accepted.json"),
        "exited" => include_str!("../fixtures/pi/agent_list.exited.json"),
        _ => panic!("unknown recorded Pi fixture"),
    };
    serde_json::from_str(raw).expect("recorded Pi fixture")
}

fn agents(name: &str) -> Value {
    fixture(name)["result"]["agents"].clone()
}

fn pi_target() -> crate::ports::agent::AgentTarget {
    let mut target = target(&source());
    target.set_test_agent_kind(HarnessKind::new("pi").expect("fixture harness"));
    target.agent_name = "pi-review".to_owned();
    target.set_test_agent_session(
        AgentSessionBinding::established("fixture/pi/session-a.jsonl").expect("fixture session"),
    );
    target
}

#[test]
fn recorded_pi_identity_readiness_and_exit_follow_the_established_session_path() {
    for (name, expected) in [
        ("established", crate::ports::agent::AgentState::Idle),
        ("working", crate::ports::agent::AgentState::Working),
    ] {
        let context = source();
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            agents(name),
            Some(("w1:p2", right_rect())),
        ));
        let targets = gateway.adjacent_targets(&context).expect("recorded Pi");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].agent_kind().as_str(), "pi");
        assert_eq!(
            targets[0].agent_session().as_id(),
            Some("fixture/pi/session-a.jsonl")
        );
        assert_eq!(targets[0].readiness, expected);
    }

    for name in ["launch_pending", "exited"] {
        let context = source();
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            agents(name),
            Some(("w1:p2", right_rect())),
        ));
        assert!(
            gateway
                .adjacent_targets(&context)
                .expect("ineligible Pi is hidden")
                .is_empty(),
            "{name}"
        );
    }
}

#[test]
fn recorded_pi_receipt_accepts_exact_submission_and_rejects_replacement() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        agents("established"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(fixture("prompted")));
    let (mut accepted_gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let prompt_text = "  quotes \"\nA\tB Grüße 第二行 e\u{301} 🧪 $(never)  ".to_owned();
    accepted_gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: pi_target(),
            content: prompt_text.clone(),
        })
        .expect("matching recorded Pi receipt");
    assert_eq!(
        runner.requests.borrow().last().expect("prompt").args[3],
        prompt_text.as_str()
    );

    let mut changed = fixture("prompted");
    changed["result"]["agent"]["agent_session"]["value"] =
        Value::String("fixture/pi/session-b.jsonl".to_owned());
    let mut responses = discovery_responses(
        &context,
        agents("established"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(changed));
    let (mut gateway, _) = gateway(responses);
    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: pi_target(),
            content: "must remain fail closed".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}
