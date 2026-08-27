use serde_json::{Value, json};

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, CLAUDE_AGENT_KIND, CLINE_AGENT_KIND,
            OPENCODE_AGENT_KIND, SubmissionRequest,
        },
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

fn with_harness(mut pane: Value, harness: &str) -> Value {
    pane["agent"] = json!(harness);
    if let Some(session) = pane
        .as_object_mut()
        .and_then(|fields| fields.get_mut("agent_session"))
    {
        session["agent"] = json!(harness);
    }
    pane
}

#[test]
fn discovery_accepts_established_sessions_for_open_ended_harness_kinds() {
    for harness in ["codex", "claude", "opencode", "future-harness"] {
        let context = source();
        let pane = with_harness(agent("w1:p2", "w1", "w1:t1", "idle"), harness);
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            json!([pane]),
            Some(("w1:p2", right_rect())),
        ));

        let targets = gateway.adjacent_targets(&context).expect("valid discovery");
        assert_eq!(targets.len(), 1, "{harness}");
        assert_eq!(targets[0].agent_kind.as_str(), harness);
        assert_eq!(targets[0].agent_session.as_id(), Some("agent-session-1"));
    }
}

#[test]
fn discovery_exposes_only_explicitly_supported_sessionless_targets() {
    for harness in ["codex", CLINE_AGENT_KIND, OPENCODE_AGENT_KIND] {
        let context = source();
        let pane = with_harness(
            without_session(agent("w1:p2", "w1", "w1:t1", "idle")),
            harness,
        );
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            json!([pane]),
            Some(("w1:p2", right_rect())),
        ));
        let targets = gateway
            .adjacent_targets(&context)
            .expect("supported empty harness");
        assert_eq!(targets.len(), 1, "{harness}");
        assert!(targets[0].agent_session.is_provisional(), "{harness}");
    }

    for harness in [CLAUDE_AGENT_KIND, "future-harness"] {
        let context = source();
        let pane = with_harness(
            without_session(agent("w1:p2", "w1", "w1:t1", "idle")),
            harness,
        );
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            json!([pane]),
            Some(("w1:p2", right_rect())),
        ));
        assert!(
            gateway
                .adjacent_targets(&context)
                .expect("unsupported sessionless harness is hidden")
                .is_empty(),
            "{harness}"
        );
    }
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
    provisional.agent_session = AgentSessionBinding::provisional();
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "start the session".to_owned(),
        })
        .expect("session-establishing prompt");

    assert_eq!(
        receipt.target.agent_session.as_id(),
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
    provisional.agent_session = AgentSessionBinding::provisional();
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
    provisional.agent_session = AgentSessionBinding::provisional();
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);

    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "accepted before the session hook".to_owned(),
        })
        .expect("matching provisional receipt");
    assert!(receipt.target.agent_session.is_provisional());
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
    provisional.agent_session = AgentSessionBinding::provisional();
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
