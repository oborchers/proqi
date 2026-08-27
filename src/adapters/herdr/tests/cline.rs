use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, AgentState, CLINE_AGENT_KIND,
            HarnessKind, SubmissionRequest,
        },
        environment::IdGenerator,
    },
};

use super::{discovery_responses, gateway, right_rect, source, success, target};

const SESSIONLESS_IDLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_list.sessionless_idle.json"
));
const ESTABLISHED_IDLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_list.established_idle.json"
));
const PROMPTED_SESSIONLESS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_prompted.sessionless_working.json"
));
const PROMPTED_ESTABLISHED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_prompted.established_working.json"
));
const REPLACED_CODEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_list.replaced_codex_idle.json"
));
const EXITED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/cline/agent_list.exited.json"
));

fn fixture(raw: &str) -> Value {
    serde_json::from_str(raw).expect("valid recorded Cline fixture")
}

fn fixture_agents(raw: &str) -> Value {
    fixture(raw)["result"]["agents"].clone()
}

fn cline_target(session: AgentSessionBinding) -> crate::ports::agent::AgentTarget {
    let context = source();
    let mut target = target(&context);
    target.agent_kind = HarnessKind::new(CLINE_AGENT_KIND).expect("fixture harness");
    target.agent_name = "cline-fixture".to_owned();
    target.agent_session = session;
    target
}

#[test]
fn recorded_cline_detection_covers_provisional_and_established_identity() {
    for (raw, expected_session) in [
        (SESSIONLESS_IDLE, None),
        (ESTABLISHED_IDLE, Some("cline-session-1")),
    ] {
        let context = source();
        let responses =
            discovery_responses(&context, fixture_agents(raw), Some(("w1:p2", right_rect())));
        let (mut gateway, _) = gateway(responses);
        let targets = gateway.adjacent_targets(&context).expect("Cline discovery");

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].agent_kind.as_str(), CLINE_AGENT_KIND);
        assert_eq!(targets[0].agent_name, "cline-fixture");
        assert_eq!(targets[0].readiness, AgentState::Idle);
        assert_eq!(targets[0].agent_session.as_id(), expected_session);
    }
}

#[test]
fn recorded_cline_identity_and_readiness_fail_closed() {
    let context = source();
    let mut inconsistent = fixture_agents(ESTABLISHED_IDLE);
    inconsistent[0]["agent_session"]["agent"] = Value::String("codex".to_owned());
    let (mut inconsistent_gateway, _) = gateway(discovery_responses(
        &context,
        inconsistent,
        Some(("w1:p2", right_rect())),
    ));
    assert!(matches!(
        inconsistent_gateway.adjacent_targets(&context),
        Err(AgentError::Malformed(_))
    ));

    for (field, value) in [
        ("agent_status", Value::String("blocked".to_owned())),
        ("interactive_ready", Value::Bool(false)),
    ] {
        let mut unavailable = fixture_agents(SESSIONLESS_IDLE);
        unavailable[0][field] = value;
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            unavailable,
            Some(("w1:p2", right_rect())),
        ));
        assert!(
            gateway
                .adjacent_targets(&context)
                .expect("unready Cline is hidden")
                .is_empty()
        );
    }
}

#[test]
fn recorded_sessionless_receipt_accepts_exact_cline_submission_once() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        fixture_agents(SESSIONLESS_IDLE),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(fixture(PROMPTED_SESSIONLESS)));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let prompt_text = "  Cline ‘exact’\nGrüße\t👩🏽‍💻\n$() ; &  ".to_owned();

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: cline_target(AgentSessionBinding::provisional()),
            content: prompt_text.clone(),
        })
        .expect("matching sessionless Cline receipt");

    assert!(receipt.target.agent_session.is_provisional());
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    let prompts = requests
        .iter()
        .filter(|request| request.args.get(1).is_some_and(|arg| arg == "prompt"))
        .collect::<Vec<_>>();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].args, ["agent", "prompt", "w1:p2", &prompt_text]);
    assert!(prompts[0].stdin.is_none());
}

#[test]
fn recorded_first_receipt_can_establish_cline_session_without_resending() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        fixture_agents(SESSIONLESS_IDLE),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(fixture(PROMPTED_ESTABLISHED)));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: cline_target(AgentSessionBinding::provisional()),
            content: "establish Cline".to_owned(),
        })
        .expect("session-establishing Cline receipt");

    assert_eq!(
        receipt.target.agent_session.as_id(),
        Some("cline-session-1")
    );
    assert_eq!(
        runner
            .requests
            .borrow()
            .iter()
            .filter(|request| request.args.get(1).is_some_and(|arg| arg == "prompt"))
            .count(),
        1
    );
}

#[test]
fn established_cline_receipt_cannot_lose_session_identity() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        fixture_agents(ESTABLISHED_IDLE),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(fixture(PROMPTED_SESSIONLESS)));
    let (mut gateway, _) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: cline_target(
                AgentSessionBinding::established("cline-session-1").expect("fixture session")
            ),
            content: "must preserve the source".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn cline_replacement_and_exit_are_detected_before_delivery() {
    let context = source();
    let replaced = discovery_responses(
        &context,
        fixture_agents(REPLACED_CODEX),
        Some(("w1:p2", right_rect())),
    );
    let (mut replacement_gateway, runner) = gateway(replaced);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    assert!(matches!(
        replacement_gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: cline_target(AgentSessionBinding::provisional()),
            content: "must not send".to_owned(),
        }),
        Err(AgentError::Unsupported(_))
    ));
    assert!(
        !runner
            .requests
            .borrow()
            .iter()
            .any(|request| request.args.get(1).is_some_and(|arg| arg == "prompt"))
    );

    let (mut gateway, _) = gateway(discovery_responses(&context, fixture_agents(EXITED), None));
    assert!(
        gateway
            .adjacent_targets(&context)
            .expect("exited Cline")
            .is_empty()
    );
}
