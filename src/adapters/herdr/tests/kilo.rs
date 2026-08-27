use std::ffi::OsString;

use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, AgentState, HarnessKind,
            SubmissionRequest,
        },
        environment::IdGenerator,
    },
};

use super::{discovery_responses, gateway, right_rect, source, success, target};

const ESTABLISHED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-list.established.json"
));
const SESSIONLESS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-list.sessionless.json"
));
const UNREADY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-list.unready.json"
));
const EXITED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-list.exited.json"
));
const REPLACED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-list.replaced.json"
));
const ACCEPTED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-prompted.accepted.json"
));
const REPLACED_RECEIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-prompted.replaced.json"
));

fn recorded(document: &str) -> Value {
    serde_json::from_str(document).expect("recorded Kilo fixture")
}

fn recorded_agents(document: &str) -> Value {
    recorded(document)["result"]["agents"].clone()
}

fn kilo_target() -> crate::ports::agent::AgentTarget {
    let context = source();
    let mut target = target(&context);
    target.agent_kind = HarnessKind::new("kilo").expect("Kilo harness kind");
    target.agent_name = "kilo-reviewer".to_owned();
    target.agent_session = AgentSessionBinding::established("ses_fixture_kilo_established")
        .expect("Kilo fixture session");
    target
}

#[test]
fn recorded_kilo_detection_uses_the_generic_established_session_path() {
    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(ESTABLISHED),
        Some(("w1:p2", right_rect())),
    );
    let (mut gateway, _) = gateway(responses);

    let targets = gateway.adjacent_targets(&context).expect("Kilo target");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].agent_kind.as_str(), "kilo");
    assert_eq!(
        targets[0].agent_session.as_id(),
        Some("ses_fixture_kilo_established")
    );
    assert_eq!(targets[0].readiness, AgentState::Idle);
}

#[test]
fn recorded_kilo_sessionless_unready_and_exit_states_stay_hidden() {
    for fixture in [SESSIONLESS, UNREADY] {
        let context = source();
        let responses = discovery_responses(
            &context,
            recorded_agents(fixture),
            Some(("w1:p2", right_rect())),
        );
        let (mut gateway, _) = gateway(responses);
        assert!(
            gateway
                .adjacent_targets(&context)
                .expect("unsupported Kilo state is hidden")
                .is_empty()
        );
    }

    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(EXITED),
        Some(("w1:p2", right_rect())),
    );
    let (mut gateway, _) = gateway(responses);
    assert!(matches!(
        gateway.adjacent_targets(&context),
        Err(AgentError::Unsupported(_))
    ));
}

#[test]
fn recorded_kilo_receipt_accepts_one_exact_semantic_prompt() {
    let pane_context = source();
    let mut responses = discovery_responses(
        &pane_context,
        recorded_agents(ESTABLISHED),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded(ACCEPTED)));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let submission_id = ids.submission_id();
    let prompt = "  quotes '\"\n\tGrüße e\u{301} 👩‍💻 $(touch never); * & |  ".to_owned();

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id,
            target: kilo_target(),
            content: prompt.clone(),
        })
        .expect("matching Kilo receipt");

    assert_eq!(receipt.submission_id, submission_id);
    assert_eq!(receipt.target.agent_kind.as_str(), "kilo");
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    let request = requests.last().expect("semantic prompt request");
    assert_eq!(request.args, ["agent", "prompt", "w1:p2", &prompt]);
    assert_eq!(request.stdin, None);
}

#[test]
fn recorded_kilo_replacement_before_delivery_sends_nothing() {
    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(REPLACED),
        Some(("w1:p2", right_rect())),
    );
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: kilo_target(),
            content: "must not be sent".to_owned(),
        }),
        Err(AgentError::Unsupported(_))
    ));
    assert!(!runner.requests.borrow().iter().any(|request| {
        request.args.first() == Some(&OsString::from("agent"))
            && request.args.get(1) == Some(&OsString::from("prompt"))
    }));
}

#[test]
fn recorded_kilo_receipt_with_a_replaced_session_fails_closed() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        recorded_agents(ESTABLISHED),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded(REPLACED_RECEIPT)));
    let (mut gateway, _) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: kilo_target(),
            content: "preserve the source".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}
