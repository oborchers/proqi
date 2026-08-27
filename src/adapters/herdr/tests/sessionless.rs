use serde_json::{Value, json};

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{AgentError, AgentGateway, CLAUDE_AGENT_KIND, SubmissionRequest},
        environment::IdGenerator,
    },
};

use super::{agent, discovery_responses, gateway, right_rect, source, success, target};

fn without_session(mut pane: Value) -> Value {
    pane.as_object_mut()
        .expect("agent object")
        .remove("agent_session");
    pane
}

#[test]
fn discovery_exposes_only_sessionless_codex_targets() {
    let context = source();
    let codex = without_session(agent("w1:p2", "w1", "w1:t1", "idle"));
    let (mut codex_gateway, _) = gateway(discovery_responses(
        &context,
        json!([codex]),
        Some(("w1:p2", right_rect())),
    ));
    let targets = codex_gateway
        .adjacent_targets(&context)
        .expect("empty Codex");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].agent_session_id, None);

    let mut claude = without_session(agent("w1:p2", "w1", "w1:t1", "idle"));
    claude["agent"] = json!(CLAUDE_AGENT_KIND);
    let (mut claude_gateway, _) = gateway(discovery_responses(
        &context,
        json!([claude]),
        Some(("w1:p2", right_rect())),
    ));
    assert!(
        claude_gateway
            .adjacent_targets(&context)
            .expect("sessionless Claude is unsupported")
            .is_empty()
    );
}

#[test]
fn first_prompt_establishes_the_session_identity() {
    let context = source();
    let empty = without_session(agent("w1:p2", "w1", "w1:t1", "idle"));
    let mut responses =
        discovery_responses(&context, json!([empty]), Some(("w1:p2", right_rect())));
    responses.push(success(json!({"result":{
        "type":"agent_prompted",
        "agent":agent("w1:p2", "w1", "w1:t1", "working")
    }})));
    let (mut gateway, runner) = gateway(responses);
    let mut provisional = target(&context);
    provisional.agent_session_id = None;
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "start the session".to_owned(),
        })
        .expect("session-establishing prompt");

    assert_eq!(
        receipt.target.agent_session_id.as_deref(),
        Some("agent-session-1")
    );
    assert_eq!(
        runner.requests.borrow().last().expect("prompt").args[1],
        "prompt"
    );
}

#[test]
fn a_session_appearing_before_submission_is_a_target_change() {
    let context = source();
    let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
    let responses = discovery_responses(&context, json!(agents), Some(("w1:p2", right_rect())));
    let (mut gateway, runner) = gateway(responses);
    let mut provisional = target(&context);
    provisional.agent_session_id = None;
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "must not send".to_owned(),
        }),
        Err(AgentError::Unsupported(_))
    ));
    assert!(!runner.requests.borrow().iter().any(|request| {
        request
            .args
            .first()
            .is_some_and(|argument| argument == "agent")
            && request
                .args
                .get(1)
                .is_some_and(|argument| argument == "prompt")
    }));
}

#[test]
fn first_prompt_receipt_may_precede_the_session_hook() {
    let context = source();
    let empty = without_session(agent("w1:p2", "w1", "w1:t1", "idle"));
    let mut responses = discovery_responses(
        &context,
        json!([empty.clone()]),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(json!({"result":{
        "type":"agent_prompted", "agent":empty
    }})));
    let (mut gateway, _) = gateway(responses);
    let mut provisional = target(&context);
    provisional.agent_session_id = None;
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "accepted before the session hook".to_owned(),
        })
        .expect("matching provisional receipt");
    assert_eq!(receipt.target.agent_session_id, None);
}

#[test]
fn inconsistent_first_session_identity_fails_closed() {
    let context = source();
    let empty = without_session(agent("w1:p2", "w1", "w1:t1", "idle"));
    let mut responses =
        discovery_responses(&context, json!([empty]), Some(("w1:p2", right_rect())));
    let mut prompted = agent("w1:p2", "w1", "w1:t1", "working");
    prompted["agent_session"]["source"] = json!("");
    responses.push(success(json!({"result":{
        "type":"agent_prompted", "agent":prompted
    }})));
    let (mut gateway, _) = gateway(responses);
    let mut provisional = target(&context);
    provisional.agent_session_id = None;
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "inconsistent receipt".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}
