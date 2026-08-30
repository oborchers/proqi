use super::*;

fn live_agent(name: Option<&str>, workspace: &str, tab: &str, pane: &str) -> Value {
    let mut value = json!({
        "agent": CODEX_AGENT_KIND,
        "agent_status": "working",
        "workspace_id": workspace,
        "tab_id": tab,
        "pane_id": pane,
        "cwd": "/private/must-not-escape",
        "terminal_title": "private prompt text",
        "terminal_id": "term_private"
    });
    if let Some(name) = name {
        value["name"] = json!(name);
    }
    value
}

fn live_snapshot(agents: &[Value], workspaces: &Value, tabs: &Value) -> FakeResponse {
    success(json!({"result":{"snapshot":{
        "protocol": 19,
        "version": "0.8.0",
        "agents": agents,
        "workspaces": workspaces,
        "tabs": tabs,
        "panes": [{"cwd":"private","terminal_title":"private"}]
    }}}))
}

#[test]
fn one_snapshot_correlates_labels_and_ignores_privacy_sensitive_fields() {
    let agents = vec![
        live_agent(Some("reviewer"), "w2", "w2:t4", "w2:p9"),
        live_agent(Some("reviewer"), "w1", "w1:t2", "w1:p8"),
        json!({
            "agent_status":"unknown","workspace_id":"w1","tab_id":"w1:t2",
            "pane_id":"w1:p-shell","terminal_title":"ordinary shell"
        }),
    ];
    let response = live_snapshot(
        &agents,
        &json!([
            {"workspace_id":"w2","label":"Second workspace"},
            {"workspace_id":"w1","label":"First workspace"}
        ]),
        &json!([
            {"workspace_id":"w2","tab_id":"w2:t4","label":"Review tab"},
            {"workspace_id":"w1","tab_id":"w1:t2","label":"Build tab"}
        ]),
    );
    let (mut gateway, runner) = gateway(vec![response]);

    let references = super::super::discovery::live_references(&mut gateway).expect("references");

    assert_eq!(references.len(), 2);
    assert_eq!(references[0].workspace_id(), "w1");
    assert_eq!(references[0].workspace_label(), Some("First workspace"));
    assert_eq!(references[0].tab_id(), "w1:t2");
    assert_eq!(references[0].tab_label(), Some("Build tab"));
    assert_eq!(references[0].pane_id(), "w1:p8");
    assert_eq!(references[0].agent_name(), "reviewer");
    assert_eq!(references[0].state(), AgentState::Working);
    let request = &runner.requests.borrow()[0];
    assert_eq!(request.args, ["api", "snapshot"]);
    assert_eq!(request.timeout, Duration::from_secs(3));
}

#[test]
fn absent_topology_labels_fall_back_to_exact_ids_and_missing_names_use_harness() {
    let response = live_snapshot(
        &[live_agent(None, "w1", "w1:t1", "w1:p1")],
        &json!([]),
        &json!([]),
    );
    let (mut labeled_gateway, _) = gateway(vec![response]);

    let references =
        super::super::discovery::live_references(&mut labeled_gateway).expect("references");

    assert_eq!(references[0].agent_name(), CODEX_AGENT_KIND);
    assert_eq!(references[0].workspace_label(), None);
    assert_eq!(references[0].tab_label(), None);

    let empty = live_snapshot(&[], &json!([]), &json!([]));
    let (mut gateway, _) = gateway(vec![empty]);
    assert!(
        super::super::discovery::live_references(&mut gateway)
            .expect("empty live references")
            .is_empty()
    );
}

#[test]
fn duplicate_or_contradictory_snapshot_identities_fail_closed() {
    let duplicate = live_agent(Some("same"), "w1", "w1:t1", "w1:p1");
    let response = live_snapshot(
        &[duplicate.clone(), duplicate],
        &json!([{"workspace_id":"w1","label":"Workspace"}]),
        &json!([{"workspace_id":"w1","tab_id":"w1:t1","label":"Tab"}]),
    );
    let (mut duplicate_gateway, _) = gateway(vec![response]);
    assert!(matches!(
        super::super::discovery::live_references(&mut duplicate_gateway),
        Err(AgentError::Ambiguous(_))
    ));

    let response = live_snapshot(
        &[live_agent(Some("agent"), "w1", "w1:t1", "w1:p1")],
        &json!([{"workspace_id":"w1","label":"Workspace"}]),
        &json!([{"workspace_id":"w2","tab_id":"w1:t1","label":"Wrong"}]),
    );
    let (mut gateway, _) = gateway(vec![response]);
    assert!(matches!(
        super::super::discovery::live_references(&mut gateway),
        Err(AgentError::Malformed(_))
    ));
}

#[test]
fn malformed_timeout_and_oversized_results_degrade_with_fixed_bounds() {
    let (mut malformed, _) = gateway(vec![success(json!({"not":"a snapshot"}))]);
    assert!(matches!(
        super::super::discovery::live_references(&mut malformed),
        Err(AgentError::Malformed(_))
    ));
    let (mut timed_out, _) = gateway(vec![FakeResponse::Error(ProcessError::TimedOut)]);
    assert_eq!(
        super::super::discovery::live_references(&mut timed_out),
        Err(AgentError::TimedOut)
    );

    let agents = (0..90)
        .map(|index| {
            live_agent(
                Some("duplicate-label"),
                "w1",
                "w1:t1",
                &format!("w1:p{index:02}"),
            )
        })
        .collect::<Vec<Value>>();
    let response = live_snapshot(&agents, &json!([]), &json!([]));
    let (mut bounded, _) = gateway(vec![response]);
    let references =
        super::super::discovery::live_references(&mut bounded).expect("bounded references");
    assert_eq!(references.len(), 64);
    assert!(references.iter().all(|reference| {
        reference.agent_name() == "duplicate-label" && reference.workspace_id() == "w1"
    }));
}

#[test]
fn topology_labels_are_sanitized_and_bounded_without_using_titles() {
    let response = live_snapshot(
        &[live_agent(Some("agent\nname"), "w1", "w1:t1", "w1:p1")],
        &json!([{"workspace_id":"w1","label":format!("{}\nsecret", "界".repeat(60))}]),
        &json!([{"workspace_id":"w1","tab_id":"w1:t1","label":"Tab\u{0007} label"}]),
    );
    let (mut gateway, _) = gateway(vec![response]);
    let references = super::super::discovery::live_references(&mut gateway).expect("references");

    assert_eq!(references[0].agent_name(), "agentname");
    assert_eq!(
        references[0]
            .workspace_label()
            .expect("workspace label")
            .chars()
            .count(),
        48
    );
    assert_eq!(references[0].tab_label(), Some("Tab label"));
}
