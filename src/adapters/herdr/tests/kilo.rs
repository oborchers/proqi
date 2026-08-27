use std::ffi::OsString;

use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, AgentState, HarnessKind,
            KILO_AGENT_KIND, SubmissionRequest,
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
const PROVISIONAL_RECEIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-prompted.provisional.json"
));
const LOST_SESSION_RECEIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/kilo/agent-prompted.lost-session.json"
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
    target.agent_kind = HarnessKind::new(KILO_AGENT_KIND).expect("Kilo harness kind");
    target.agent_name = "kilo-reviewer".to_owned();
    target.agent_session = AgentSessionBinding::established("ses_fixture_kilo_established")
        .expect("Kilo fixture session");
    target
}

fn provisional_kilo_target() -> crate::ports::agent::AgentTarget {
    let mut target = kilo_target();
    target.agent_session = AgentSessionBinding::provisional();
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
    assert_eq!(targets[0].agent_kind.as_str(), KILO_AGENT_KIND);
    assert_eq!(
        targets[0].agent_session.as_id(),
        Some("ses_fixture_kilo_established")
    );
    assert_eq!(targets[0].readiness, AgentState::Idle);
}

#[test]
fn recorded_kilo_sessionless_is_provisional_while_unready_and_exit_stay_hidden() {
    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(SESSIONLESS),
        Some(("w1:p2", right_rect())),
    );
    let (mut provisional_gateway, _) = gateway(responses);
    let targets = provisional_gateway
        .adjacent_targets(&context)
        .expect("provisional Kilo target");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].agent_kind.as_str(), KILO_AGENT_KIND);
    assert!(targets[0].agent_session.is_provisional());

    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(UNREADY),
        Some(("w1:p2", right_rect())),
    );
    let (mut unready_gateway, _) = gateway(responses);
    assert!(
        unready_gateway
            .adjacent_targets(&context)
            .expect("unready Kilo state is hidden")
            .is_empty()
    );

    let context = source();
    let responses = discovery_responses(
        &context,
        recorded_agents(EXITED),
        Some(("w1:p2", right_rect())),
    );
    let (mut exited_gateway, _) = gateway(responses);
    assert!(
        exited_gateway
            .adjacent_targets(&context)
            .expect("exited Kilo target is absent")
            .is_empty()
    );
}

#[test]
fn recorded_kilo_first_receipt_may_establish_the_session() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        recorded_agents(SESSIONLESS),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded(ACCEPTED)));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional_kilo_target(),
            content: "first Kilo prompt".to_owned(),
        })
        .expect("session-establishing Kilo receipt");

    assert_eq!(
        receipt.target.agent_session.as_id(),
        Some("ses_fixture_kilo_established")
    );
    assert_eq!(semantic_prompt_count(&runner.requests.borrow()), 1);
}

#[test]
fn recorded_kilo_first_receipt_may_precede_the_session_hook() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        recorded_agents(SESSIONLESS),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded(PROVISIONAL_RECEIPT)));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional_kilo_target(),
            content: "first Kilo prompt before hook".to_owned(),
        })
        .expect("matching provisional Kilo receipt");

    assert!(receipt.target.agent_session.is_provisional());
    assert_eq!(semantic_prompt_count(&runner.requests.borrow()), 1);
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
    assert_eq!(receipt.target.agent_kind.as_str(), KILO_AGENT_KIND);
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

#[test]
fn recorded_established_kilo_receipt_without_a_session_fails_closed() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        recorded_agents(ESTABLISHED),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded(LOST_SESSION_RECEIPT)));
    let (mut gateway, _) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: kilo_target(),
            content: "preserve after lost identity".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}

fn semantic_prompt_count(requests: &[crate::ports::environment::ProcessRequest]) -> usize {
    requests
        .iter()
        .filter(|request| {
            request.args.first() == Some(&OsString::from("agent"))
                && request.args.get(1) == Some(&OsString::from("prompt"))
        })
        .count()
}
