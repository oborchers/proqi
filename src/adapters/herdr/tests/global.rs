use super::*;

use crate::ports::agent::{AgentAvailability, SubmissionRoute};

fn live_agent(
    workspace: &str,
    tab: &str,
    pane: &str,
    name: &str,
    state: &str,
    session: Option<&str>,
) -> Value {
    let mut value = json!({
        "workspace_id": workspace,
        "tab_id": tab,
        "pane_id": pane,
        "agent": CODEX_AGENT_KIND,
        "name": name,
        "agent_status": state,
        "interactive_ready": true,
        "launch_pending": false
    });
    if let Some(session) = session {
        value["agent_session"] = json!({
            "agent": CODEX_AGENT_KIND,
            "kind": "id",
            "source": "herdr:codex",
            "value": session
        });
    }
    value
}

fn global_snapshot(protocol: u32, agents: &[Value]) -> Value {
    json!({"result":{"snapshot":{
        "protocol": protocol,
        "version": fixture_version(protocol),
        "workspaces":[
            {"workspace_id":"w1","label":"Main 世界"},
            {"workspace_id":"w2","label":"Other"}
        ],
        "tabs":[
            {"workspace_id":"w1","tab_id":"w1:t1","label":"Board"},
            {"workspace_id":"w1","tab_id":"w1:t2","label":"Review"},
            {"workspace_id":"w2","tab_id":"w2:t1","label":"Remote local"}
        ],
        "agents": agents
    }}})
}

fn global_responses(protocol: u32, agents: &[Value]) -> Vec<FakeResponse> {
    vec![
        success(schema(protocol)),
        success(global_snapshot(protocol, agents)),
        success(current(&source())),
    ]
}

#[test]
fn current_server_discovery_keeps_cross_tab_workspace_and_disabled_states_truthful() {
    let agents = vec![
        live_agent("w1", "w1:t1", "w1:p1", "self", "idle", Some("self")),
        live_agent("w1", "w1:t2", "w1:p2", "同名", "idle", Some("s1")),
        live_agent("w2", "w2:t1", "w2:p8", "同名", "done", Some("s2")),
        live_agent("w1", "w1:t2", "w1:p3", "busy", "working", Some("s3")),
        live_agent("w1", "w1:t2", "w1:p4", "blocked", "blocked", Some("s4")),
        live_agent("w1", "w1:t2", "w1:p5", "unknown", "unknown", Some("s5")),
        {
            let mut agent = live_agent("w1", "w1:t2", "w1:p6", "launching", "idle", Some("s6"));
            agent["launch_pending"] = json!(true);
            agent
        },
        {
            let mut agent = live_agent(
                "w1",
                "w1:t2",
                "w1:p7",
                "not interactive",
                "idle",
                Some("s7"),
            );
            agent["interactive_ready"] = json!(false);
            agent
        },
    ];
    let (mut gateway, runner) = gateway(global_responses(20, &agents));
    let targets = gateway.global_targets().expect("global targets");

    assert_eq!(targets.len(), 7);
    assert!(
        targets
            .iter()
            .all(|target| matches!(target.route, SubmissionRoute::HerdrAgent(_)))
    );
    assert_eq!(
        targets.iter().filter(|target| target.can_submit()).count(),
        3
    );
    assert!(targets.iter().any(|target| {
        target.workspace_id() == "w2"
            && target.tab_id() == "w2:t1"
            && target.readiness == AgentState::Done
            && target.workspace_label.as_deref() == Some("Other")
    }));
    assert_eq!(
        targets
            .iter()
            .find(|target| target.agent_name == "blocked")
            .map(|target| target.availability),
        Some(AgentAvailability::Blocked)
    );
    assert_eq!(
        targets
            .iter()
            .find(|target| target.agent_name == "unknown")
            .map(|target| target.availability),
        Some(AgentAvailability::Unknown)
    );
    assert_eq!(
        targets
            .iter()
            .find(|target| target.agent_name == "launching")
            .map(|target| target.availability),
        Some(AgentAvailability::Launching)
    );
    assert_eq!(
        targets
            .iter()
            .find(|target| target.agent_name == "not interactive")
            .map(|target| target.availability),
        Some(AgentAvailability::NotInteractive)
    );
    let requests = runner.requests.borrow();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].args, ["api", "schema", "--json"]);
    assert_eq!(requests[1].args, ["api", "snapshot"]);
    assert_eq!(requests[2].args, ["pane", "current", "--current"]);
    assert!(targets.iter().all(|target| target.pane_id() != "w1:p1"));
}

#[test]
fn duplicate_pane_identity_and_incomplete_topology_fail_closed() {
    let duplicate = live_agent("w1", "w1:t2", "w1:p2", "one", "idle", Some("s1"));
    let (mut duplicate_gateway, _) = gateway(global_responses(20, &[duplicate.clone(), duplicate]));
    assert!(matches!(
        duplicate_gateway.global_targets(),
        Err(AgentError::Ambiguous(_))
    ));

    let incomplete = live_agent(
        "missing",
        "missing:t1",
        "missing:p1",
        "one",
        "idle",
        Some("s1"),
    );
    let (mut gateway, _) = gateway(global_responses(20, &[incomplete]));
    assert!(matches!(
        gateway.global_targets(),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn global_submission_revalidates_exact_address_and_accepts_label_renames() {
    let initial = live_agent("w2", "w2:t1", "w2:p8", "before", "idle", Some("s1"));
    let renamed = live_agent("w2", "w2:t1", "w2:p8", "after", "working", Some("s1"));
    let mut responses = global_responses(20, &[initial]);
    let mut renamed_snapshot = global_snapshot(20, std::slice::from_ref(&renamed));
    renamed_snapshot["result"]["snapshot"]["workspaces"][1]["label"] = json!("Renamed workspace");
    renamed_snapshot["result"]["snapshot"]["tabs"][2]["label"] = json!("Renamed tab");
    responses.push(success(schema(20)));
    responses.push(success(renamed_snapshot));
    responses.push(success(current(&source())));
    responses.push(success(json!({"result":{
        "type":"agent_prompted",
        "agent":renamed
    }})));
    let (mut gateway, runner) = gateway(responses);
    let target = gateway.global_targets().expect("initial target").remove(0);
    let mut ids = FakeIdGenerator::new(1_900);
    let request = SubmissionRequest {
        submission_id: ids.submission_id(),
        target,
        content: "control\u{1b}[31m\nGrüße".to_owned(),
    };
    let receipt = gateway.submit(request.clone()).expect("accepted prompt");

    assert_eq!(receipt.target.agent_name, "after");
    assert_eq!(
        receipt.target.workspace_label.as_deref(),
        Some("Renamed workspace")
    );
    assert_eq!(receipt.post_state, Some(AgentState::Working));
    let requests = runner.requests.borrow();
    assert_eq!(requests[6].args[0..3], ["agent", "prompt", "w2:p8"]);
    assert_eq!(requests[6].args[3], request.content.as_str());
}

#[test]
fn movement_disappearance_replacement_and_disabled_refresh_send_no_prompt() {
    for refreshed in [
        Vec::new(),
        vec![live_agent(
            "w1",
            "w1:t2",
            "w1:p2",
            "moved",
            "idle",
            Some("s1"),
        )],
        vec![live_agent(
            "w2",
            "w2:t1",
            "w2:p8",
            "replaced",
            "idle",
            Some("s2"),
        )],
        vec![live_agent(
            "w2",
            "w2:t1",
            "w2:p8",
            "blocked",
            "blocked",
            Some("s1"),
        )],
    ] {
        let initial = live_agent("w2", "w2:t1", "w2:p8", "initial", "idle", Some("s1"));
        let mut responses = global_responses(20, &[initial]);
        responses.extend(global_responses(20, &refreshed));
        let (mut gateway, runner) = gateway(responses);
        let target = gateway.global_targets().expect("initial target").remove(0);
        let mut ids = FakeIdGenerator::new(2_000);
        let request = SubmissionRequest {
            submission_id: ids.submission_id(),
            target,
            content: "exact".to_owned(),
        };
        assert!(matches!(
            gateway.submit(request),
            Err(AgentError::Unsupported(_))
        ));
        assert_eq!(runner.requests.borrow().len(), 6);
    }
}

#[test]
fn receipt_mismatch_after_structured_prompt_is_never_accepted() {
    let initial = live_agent("w2", "w2:t1", "w2:p8", "initial", "idle", Some("s1"));
    let mismatched = live_agent("w2", "w2:t1", "w2:p9", "other", "working", Some("s1"));
    let mut responses = global_responses(20, std::slice::from_ref(&initial));
    responses.extend(global_responses(20, &[initial]));
    responses.push(success(json!({"result":{
        "type":"agent_prompted",
        "agent":mismatched
    }})));
    let (mut gateway, _) = gateway(responses);
    let target = gateway.global_targets().expect("initial target").remove(0);
    let mut ids = FakeIdGenerator::new(2_100);
    let request = SubmissionRequest {
        submission_id: ids.submission_id(),
        target,
        content: "exact".to_owned(),
    };
    assert!(matches!(
        gateway.submit(request),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn protocol_change_during_revalidation_sends_no_prompt() {
    let initial = live_agent("w2", "w2:t1", "w2:p8", "initial", "idle", Some("s1"));
    let mut responses = global_responses(20, std::slice::from_ref(&initial));
    responses.extend(global_responses(21, &[initial]));
    let (mut gateway, runner) = gateway(responses);
    let target = gateway.global_targets().expect("initial target").remove(0);
    let mut ids = FakeIdGenerator::new(2_200);
    let request = SubmissionRequest {
        submission_id: ids.submission_id(),
        target,
        content: "exact".to_owned(),
    };

    assert!(matches!(
        gateway.submit(request),
        Err(AgentError::Unsupported(_))
    ));
    assert_eq!(runner.requests.borrow().len(), 6);
}
