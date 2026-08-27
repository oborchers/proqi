use std::{cell::RefCell, collections::VecDeque, ffi::OsString, rc::Rc, time::Duration};

use serde_json::{Value, json};

use crate::{
    adapters::memory::FakeIdGenerator,
    domain::Direction,
    ports::{
        agent::{
            AgentError, AgentGateway, AgentSessionBinding, AgentState, AgentTarget,
            CODEX_AGENT_KIND, HarnessKind, PaneContext, PanePresentation, PaneRect,
            SubmissionRequest,
        },
        environment::{IdGenerator, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner},
    },
};

use super::HerdrGateway;

#[path = "tests/cline.rs"]
mod cline;
#[path = "tests/hermes.rs"]
mod hermes;
#[path = "tests/opencode.rs"]
mod opencode;
#[path = "tests/pi.rs"]
mod pi;
#[path = "tests/sessionless.rs"]
mod sessionless;
#[path = "tests/submission_receipts.rs"]
mod submission_receipts;

#[derive(Clone, Default)]
struct FakeRunner {
    responses: Rc<RefCell<VecDeque<FakeResponse>>>,
    requests: Rc<RefCell<Vec<ProcessRequest>>>,
}

impl FakeRunner {
    fn with(responses: Vec<FakeResponse>) -> Self {
        Self {
            responses: Rc::new(RefCell::new(responses.into())),
            requests: Rc::default(),
        }
    }
}

impl ProcessRunner for FakeRunner {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        self.requests.borrow_mut().push(request);
        match self
            .responses
            .borrow_mut()
            .pop_front()
            .expect("recorded Herdr response")
        {
            FakeResponse::Output(output) => Ok(output),
            FakeResponse::Error(error) => Err(error),
        }
    }
}

enum FakeResponse {
    Output(ProcessOutput),
    Error(ProcessError),
}

fn success(value: Value) -> FakeResponse {
    let stdout = serde_json::to_vec(&value).expect("fixture JSON");
    drop(value);
    FakeResponse::Output(ProcessOutput {
        exit_code: Some(0),
        stdout,
        stderr: Vec::new(),
    })
}

fn source() -> PaneContext {
    PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        },
    }
}

fn schema(protocol: u32) -> Value {
    json!({
        "protocol": protocol,
        "schema_version": 1,
        "schemas": {
            "request": {"const": "agent.prompt"},
            "response": {"const": "agent_prompted"}
        }
    })
}

fn snapshot(protocol: u32) -> Value {
    json!({"result":{"snapshot":{"protocol":protocol,"version":"0.8.0"}}})
}

fn current(context: &PaneContext) -> Value {
    json!({"result":{"pane":{
        "pane_id":context.pane_id,"workspace_id":context.workspace_id,"tab_id":context.tab_id
    }}})
}

fn layout(context: &PaneContext) -> Value {
    json!({"result":{"layout":layout_value(context, &[])}})
}

fn layout_value(context: &PaneContext, extra: &[(&str, PaneRect)]) -> Value {
    let mut panes = vec![json!({"pane_id":context.pane_id,"rect":rect(context.rect)})];
    panes.extend(
        extra
            .iter()
            .map(|(id, area)| json!({"pane_id":id,"rect":rect(*area)})),
    );
    json!({"workspace_id":context.workspace_id,"tab_id":context.tab_id,"panes":panes})
}

fn rect(area: PaneRect) -> Value {
    json!({"x":area.x,"y":area.y,"width":area.width,"height":area.height})
}

fn capability_responses(context: &PaneContext) -> Vec<FakeResponse> {
    vec![
        success(schema(19)),
        success(snapshot(19)),
        success(current(context)),
        success(layout(context)),
    ]
}

fn right_rect() -> PaneRect {
    PaneRect {
        x: 30,
        y: 15,
        width: 12,
        height: 10,
    }
}

fn up_rect() -> PaneRect {
    PaneRect {
        x: 15,
        y: 0,
        width: 10,
        height: 10,
    }
}

fn agent(pane_id: &str, workspace: &str, tab: &str, status: &str) -> Value {
    json!({
        "pane_id":pane_id,"workspace_id":workspace,"tab_id":tab,
        "agent":CODEX_AGENT_KIND,"name":"reviewer","agent_status":status,
        "agent_session":{"agent":CODEX_AGENT_KIND,"kind":"id","source":"herdr:codex","value":"agent-session-1"}
    })
}

fn neighbor(
    context: &PaneContext,
    direction: Direction,
    candidate: Option<(&str, PaneRect)>,
) -> Value {
    let extras = candidate.into_iter().collect::<Vec<_>>();
    json!({"result":{"neighbor":{
        "pane_id":context.pane_id,"direction":direction,
        "neighbor_pane_id":candidate.map(|(id, _)| id),
        "layout":layout_value(context, &extras)
    }}})
}

fn discovery_responses(
    context: &PaneContext,
    agents: Value,
    right: Option<(&str, PaneRect)>,
) -> Vec<FakeResponse> {
    let mut responses = capability_responses(context);
    responses.push(success(json!({"result":{"agents":agents}})));
    drop(agents);
    responses.push(success(neighbor(context, Direction::Up, None)));
    responses.push(success(neighbor(context, Direction::Right, right)));
    responses.push(success(neighbor(context, Direction::Down, None)));
    responses.push(success(neighbor(context, Direction::Left, None)));
    responses
}

fn gateway(responses: Vec<FakeResponse>) -> (HerdrGateway<FakeRunner>, FakeRunner) {
    let runner = FakeRunner::with(responses);
    (
        HerdrGateway::new(OsString::from("herdr-fixture"), runner.clone(), true),
        runner,
    )
}

#[test]
fn capability_negotiation_verifies_live_protocol_and_current_geometry() {
    let context = source();
    let (mut gateway, runner) = gateway(capability_responses(&context));
    let capability = gateway.capabilities().expect("capability");
    assert_eq!(capability.protocol, 19);
    assert_eq!(
        capability.delivery,
        crate::ports::agent::AgentDeliveryCapabilities::SUBMIT_ONLY
    );
    assert_eq!(capability.version, "0.8.0");
    assert_eq!(capability.context, context);
    let requests = runner.requests.borrow();
    assert_eq!(requests[0].args, ["api", "schema", "--json"]);
    assert_eq!(requests[1].args, ["api", "snapshot"]);
    assert_eq!(requests[2].args, ["pane", "current", "--current"]);
}

#[test]
fn all_directions_are_queried_and_only_independently_verified_agents_return() {
    let context = source();
    let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
    let (mut gateway, runner) = gateway(discovery_responses(
        &context,
        json!(agents),
        Some(("w1:p2", right_rect())),
    ));
    let targets = gateway.adjacent_targets(&context).expect("targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].direction, Direction::Right);
    assert_eq!(targets[0].readiness, AgentState::Idle);
    let requests = runner.requests.borrow();
    let directional = requests
        .iter()
        .filter(|request| request.args.get(1) == Some(&OsString::from("neighbor")))
        .collect::<Vec<_>>();
    assert_eq!(directional.len(), 4);
}

#[test]
fn ordinary_neighbor_without_agent_identity_does_not_hide_a_valid_target() {
    let context = source();
    let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
    let mut responses = capability_responses(&context);
    responses.push(success(json!({"result":{"agents":agents}})));
    responses.push(success(neighbor(
        &context,
        Direction::Up,
        Some(("w1:p-shell", up_rect())),
    )));
    responses.push(success(neighbor(
        &context,
        Direction::Right,
        Some(("w1:p2", right_rect())),
    )));
    responses.push(success(neighbor(&context, Direction::Down, None)));
    responses.push(success(neighbor(&context, Direction::Left, None)));
    let (mut gateway, _) = gateway(responses);

    let targets = gateway
        .adjacent_targets(&context)
        .expect("ordinary shell is ignored");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].direction, Direction::Right);
}

#[test]
fn protocol_mismatch_timeout_and_malformed_output_fail_closed() {
    let (mut mismatch, _) = gateway(vec![success(schema(18)), success(snapshot(18))]);
    assert!(matches!(
        mismatch.capabilities(),
        Err(AgentError::Unsupported(_))
    ));
    let (mut timed_out, _) = gateway(vec![FakeResponse::Error(ProcessError::TimedOut)]);
    assert_eq!(timed_out.capabilities(), Err(AgentError::TimedOut));
    let (mut malformed, _) = gateway(vec![success(json!({"not":"schema"}))]);
    assert!(matches!(
        malformed.capabilities(),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn ambiguity_wrong_context_invalid_geometry_and_unsupported_state_are_rejected() {
    let context = source();
    let duplicate = agent("w1:p2", "w1", "w1:t1", "idle");
    let (mut ambiguous, _) = gateway(discovery_responses(
        &context,
        json!([duplicate.clone(), duplicate]),
        Some(("w1:p2", right_rect())),
    ));
    assert!(matches!(
        ambiguous.adjacent_targets(&context),
        Err(AgentError::Ambiguous(_))
    ));

    let wrong = vec![agent("w1:p2", "w2", "w1:t1", "idle")];
    let (mut wrong_context, _) = gateway(discovery_responses(
        &context,
        json!(wrong),
        Some(("w1:p2", right_rect())),
    ));
    assert!(matches!(
        wrong_context.adjacent_targets(&context),
        Err(AgentError::Malformed(_))
    ));

    let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
    let invalid = PaneRect {
        x: 31,
        ..right_rect()
    };
    let (mut geometry, _) = gateway(discovery_responses(
        &context,
        json!(agents),
        Some(("w1:p2", invalid)),
    ));
    assert!(matches!(
        geometry.adjacent_targets(&context),
        Err(AgentError::Malformed(_))
    ));

    let blocked = vec![agent("w1:p2", "w1", "w1:t1", "blocked")];
    let (mut unsupported, _) = gateway(discovery_responses(
        &context,
        json!(blocked),
        Some(("w1:p2", right_rect())),
    ));
    assert!(
        unsupported
            .adjacent_targets(&context)
            .expect("unsupported neighbor is hidden")
            .is_empty()
    );
}

#[test]
fn explicit_interactive_readiness_metadata_fails_closed() {
    let context = source();
    for (field, value) in [("interactive_ready", false), ("launch_pending", true)] {
        let mut unavailable = agent("w1:p2", "w1", "w1:t1", "idle");
        unavailable[field] = json!(value);
        let (mut gateway, _) = gateway(discovery_responses(
            &context,
            json!([unavailable]),
            Some(("w1:p2", right_rect())),
        ));
        assert!(
            gateway
                .adjacent_targets(&context)
                .expect("unready neighbor is hidden")
                .is_empty()
        );
    }
}

fn target(context: &PaneContext) -> AgentTarget {
    AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 19,
        direction: Direction::Right,
        pane_id: "w1:p2".to_owned(),
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        agent_kind: HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        agent_name: "reviewer".to_owned(),
        agent_session: AgentSessionBinding::established("agent-session-1")
            .expect("fixture session"),
        readiness: AgentState::Idle,
        delivery: crate::ports::agent::AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: right_rect(),
        source: context.clone(),
    }
}

#[test]
fn submission_revalidates_and_passes_exact_text_as_one_distinct_argument() {
    let context = source();
    let agents = vec![agent("w1:p2", "w1", "w1:t1", "idle")];
    let mut responses = discovery_responses(&context, json!(agents), Some(("w1:p2", right_rect())));
    responses.push(success(json!({"result":{
        "type":"agent_prompted",
        "agent":agent("w1:p2", "w1", "w1:t1", "working")
    }})));
    let (mut gateway, runner) = gateway(responses);
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let submission_id = ids.submission_id();
    let prompt_text = "$(touch never); Grüße\n第二行".to_owned();
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id,
            target: target(&context),
            content: prompt_text.clone(),
        })
        .expect("accepted submission");
    assert_eq!(receipt.submission_id, submission_id);
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    let prompt = requests.last().expect("prompt request");
    assert_eq!(prompt.program, OsString::from("herdr-fixture"));
    assert_eq!(
        prompt.args,
        ["agent", "prompt", "w1:p2", prompt_text.as_str()]
    );
    assert_eq!(prompt.stdin, None);
    assert_eq!(prompt.timeout, Duration::from_secs(5));
}

#[test]
fn capability_negotiation_rejects_a_schema_without_semantic_prompt_contracts() {
    let context = source();
    let incomplete = json!({"protocol":19,"schema_version":1,"schemas":{}});
    let (mut gateway, _) = gateway(vec![
        success(incomplete),
        success(snapshot(19)),
        success(current(&context)),
        success(layout(&context)),
    ]);
    assert!(matches!(
        gateway.capabilities(),
        Err(AgentError::Unsupported(_))
    ));
}

#[test]
fn unmanaged_environment_never_executes_herdr() {
    let runner = FakeRunner::default();
    let mut gateway = HerdrGateway::new(OsString::from("herdr"), runner.clone(), false);
    assert!(matches!(
        gateway.capabilities(),
        Err(AgentError::Unavailable(_))
    ));
    assert!(runner.requests.borrow().is_empty());
}

#[test]
fn pane_identity_uses_display_only_metadata_with_ttl_and_clean_clear() {
    let responses = vec![success(json!({})), success(json!({}))];
    let (mut gateway, runner) = gateway(responses);
    gateway
        .publish("w1:p1", 7, Duration::from_secs(15))
        .expect("publish display metadata");
    gateway.clear("w1:p1", 8).expect("clear display metadata");

    let requests = runner.requests.borrow();
    assert_eq!(
        requests[0].args,
        [
            "pane",
            "report-metadata",
            "w1:p1",
            "--source",
            "proqi",
            "--title",
            "proqi",
            "--display-agent",
            "proqi",
            "--seq",
            "7",
            "--ttl-ms",
            "15000",
        ]
    );
    assert!(!requests[0].args.contains(&OsString::from("--agent")));
    assert_eq!(
        requests[1].args,
        [
            "pane",
            "report-metadata",
            "w1:p1",
            "--source",
            "proqi",
            "--clear-title",
            "--clear-display-agent",
            "--seq",
            "8",
        ]
    );
}

#[test]
fn pane_identity_can_use_a_process_unique_monotonic_source() {
    let responses = vec![success(json!({}))];
    let runner = FakeRunner::with(responses);
    let mut gateway = HerdrGateway::new(OsString::from("herdr"), runner.clone(), true)
        .with_presentation_source("proqi-ins_example".to_owned());

    gateway
        .publish("w1:p1", 1, Duration::from_secs(15))
        .expect("publish display metadata");

    let requests = runner.requests.borrow();
    assert_eq!(requests[0].args[3..5], ["--source", "proqi-ins_example"]);
}
