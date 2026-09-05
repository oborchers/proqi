//! Sanitized recordings from `OpenCode` 1.18.23 under Herdr 0.8.0 protocol 19.

use std::ffi::OsString;

use serde_json::Value;

use crate::{
    adapters::memory::FakeIdGenerator,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, AgentState, HarnessKind,
            OPENCODE_AGENT_KIND, SubmissionRequest,
        },
        environment::IdGenerator,
    },
};

use super::{FakeResponse, discovery_responses, gateway, right_rect, source, success, target};

fn recorded(name: &str) -> Value {
    let content = match name {
        "sessionless" => include_str!("fixtures/opencode/agent_list.sessionless.json"),
        "idle" => include_str!("fixtures/opencode/agent_list.established_idle.json"),
        "working" => include_str!("fixtures/opencode/agent_list.established_working.json"),
        "unready" => include_str!("fixtures/opencode/agent_list.unready.json"),
        "replaced" => include_str!("fixtures/opencode/agent_list.replaced_session.json"),
        "exited" => include_str!("fixtures/opencode/agent_list.exited.json"),
        "accepted" => include_str!("fixtures/opencode/prompt.accepted.json"),
        "before_hook" => include_str!("fixtures/opencode/prompt.before_session_hook.json"),
        "lost_session" => include_str!("fixtures/opencode/prompt.lost_session.json"),
        "replaced_receipt" => include_str!("fixtures/opencode/prompt.replaced_session.json"),
        _ => panic!("unknown recorded OpenCode fixture"),
    };
    serde_json::from_str(content).expect("valid sanitized OpenCode recording")
}

fn agents(name: &str) -> Value {
    recorded(name)["result"]["agents"].clone()
}

fn opencode_target(context: &crate::ports::agent::PaneContext) -> crate::ports::agent::AgentTarget {
    let mut result = target(context);
    result.set_test_agent_kind(HarnessKind::new(OPENCODE_AGENT_KIND).expect("fixture harness"));
    result.agent_name = "opencode-fixture".to_owned();
    result.set_test_agent_session(
        AgentSessionBinding::established("opencode-session-alpha").expect("fixture session"),
    );
    result
}

fn established_discovery(name: &str) -> Vec<FakeResponse> {
    let context = source();
    discovery_responses(&context, agents(name), Some(("w1:p2", right_rect())))
}

#[test]
fn recorded_detection_requires_established_opencode_identity_and_readiness() {
    let context = source();
    for (name, expected, provisional) in [
        ("sessionless", Some(AgentState::Idle), true),
        ("idle", Some(AgentState::Idle), false),
        ("working", Some(AgentState::Working), false),
        ("unready", None, false),
    ] {
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            agents(name),
            Some(("w1:p2", right_rect())),
        ));
        let targets = gateway
            .adjacent_targets(&context)
            .expect("recorded discovery");
        assert_eq!(
            targets.first().map(|target| target.readiness),
            expected,
            "{name}"
        );
        if let Some(target) = targets.first() {
            assert_eq!(target.agent_kind().as_str(), OPENCODE_AGENT_KIND);
            assert_eq!(target.agent_session().is_provisional(), provisional);
            if !provisional {
                assert_eq!(
                    target.agent_session().as_id(),
                    Some("opencode-session-alpha")
                );
            }
        }
    }
}

#[test]
fn recorded_exit_removes_the_opencode_target() {
    let context = source();
    let (mut live, _) = gateway(established_discovery("idle"));
    assert_eq!(
        live.adjacent_targets(&context).expect("live target").len(),
        1
    );

    let (mut exited, _) = gateway(discovery_responses(&context, agents("exited"), None));
    assert!(
        exited
            .adjacent_targets(&context)
            .expect("exited target")
            .is_empty()
    );
}

#[test]
fn recorded_receipt_accepts_exact_opencode_submission_as_one_argument() {
    let context = source();
    let mut responses = established_discovery("idle");
    responses.push(success(recorded("accepted")));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let payload = "  quotes '\"'\n\ttabs e\u{301} 👩‍💻 第二行\n$(touch never)  ".to_owned();
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: opencode_target(&context),
            content: payload.clone(),
        })
        .expect("matching OpenCode receipt");

    assert_eq!(receipt.target.agent_kind().as_str(), OPENCODE_AGENT_KIND);
    assert_eq!(
        receipt.target.agent_session().as_id(),
        Some("opencode-session-alpha")
    );
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    let prompt = requests.last().expect("semantic prompt request");
    assert_eq!(prompt.args, ["agent", "prompt", "w1:p2", payload.as_str()]);
    assert_eq!(prompt.stdin, None);
}

#[test]
fn first_opencode_prompt_is_exactly_once_and_establishes_the_recorded_session() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        agents("sessionless"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded("accepted")));
    let (mut gateway, runner) = gateway(responses);
    let mut provisional = opencode_target(&context);
    provisional.set_test_agent_session(AgentSessionBinding::provisional());
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let payload = "first OpenCode prompt\nwith Unicode e\u{301} 👩‍💻".to_owned();
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: payload.clone(),
        })
        .expect("session-establishing OpenCode receipt");

    assert_eq!(
        receipt.target.agent_session().as_id(),
        Some("opencode-session-alpha")
    );
    let prompts = runner
        .requests
        .borrow()
        .iter()
        .filter(|request| request.args.get(1) == Some(&OsString::from("prompt")))
        .map(|request| request.args.clone())
        .collect::<Vec<_>>();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0][3], payload.as_str());
}

#[test]
fn first_opencode_receipt_may_precede_the_session_hook_without_resending() {
    let context = source();
    let mut responses = discovery_responses(
        &context,
        agents("sessionless"),
        Some(("w1:p2", right_rect())),
    );
    responses.push(success(recorded("before_hook")));
    let (mut gateway, runner) = gateway(responses);
    let mut provisional = opencode_target(&context);
    provisional.set_test_agent_session(AgentSessionBinding::provisional());
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: provisional,
            content: "accepted before hook".to_owned(),
        })
        .expect("matching provisional OpenCode receipt");

    assert!(receipt.target.agent_session().is_provisional());
    assert_eq!(
        runner
            .requests
            .borrow()
            .iter()
            .filter(|request| request.args.get(1) == Some(&OsString::from("prompt")))
            .count(),
        1
    );
}

#[test]
fn opencode_replacement_before_delivery_sends_nothing() {
    let context = source();
    let (mut gateway, runner) = gateway(established_discovery("replaced"));
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: opencode_target(&context),
            content: "must not send".to_owned(),
        }),
        Err(AgentError::Unsupported(_))
    ));
    assert!(!runner.requests.borrow().iter().any(|request| {
        request.args.get(0..2) == Some(&[OsString::from("agent"), OsString::from("prompt")])
    }));
}

#[test]
fn opencode_receipt_with_a_different_session_fails_closed() {
    let context = source();
    let mut responses = established_discovery("idle");
    responses.push(success(recorded("replaced_receipt")));
    let (mut gateway, _) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: opencode_target(&context),
            content: "preserve on mismatch".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn established_opencode_receipt_that_loses_session_identity_fails_closed() {
    let context = source();
    let mut responses = established_discovery("idle");
    responses.push(success(recorded("lost_session")));
    let (mut gateway, _) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    assert!(matches!(
        gateway.submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: opencode_target(&context),
            content: "preserve when identity disappears".to_owned(),
        }),
        Err(AgentError::Malformed(_))
    ));
}
