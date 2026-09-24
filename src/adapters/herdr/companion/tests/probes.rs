//! Process probes: a bounded budget, and unknown results that never enable a close.

use std::time::Duration;

use serde_json::json;

use crate::ports::{
    companion::{CompanionHost, PaneProcess},
    environment::ProcessOutput,
};

use super::{ScriptedRunner, host, info, ok, plugin_context};

fn failure() -> ProcessOutput {
    ProcessOutput {
        exit_code: None,
        stdout: Vec::new(),
        stderr: b"timed out".to_vec(),
    }
}

#[test]
fn a_failed_or_timed_out_probe_reports_unknown_instead_of_aborting() {
    let runner = ScriptedRunner::with(vec![failure()]);
    let process = host(&runner, &plugin_context())
        .process("w1:p3")
        .expect("unknown, not an error");
    assert_eq!(process, Some(PaneProcess::Unknown));
}

#[test]
fn an_unrelated_failing_pane_does_not_abort_the_tab_snapshot() {
    let runner = ScriptedRunner::with(vec![
        ok(&json!({ "type": "pane_list", "panes": [
            { "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "codex" },
            { "pane_id": "w1:p4", "tab_id": "w1:t1" },
            { "pane_id": "w1:p5", "tab_id": "w1:t1" }
        ]})),
        failure(),
        info(9, 11, &[(11, &["proqi", "--resume", super::SESSION])]),
    ]);
    let panes = host(&runner, &plugin_context())
        .tab_panes("w1:t1")
        .expect("snapshot");
    let present: Vec<_> = panes.iter().map(|pane| pane.proqi_presence).collect();
    assert_eq!(present, vec![false, false, true]);
}

#[test]
fn probes_after_the_window_closes_report_unknown_without_calling_herdr() {
    let runner = ScriptedRunner::with(Vec::new());
    let mut host = host(&runner, &plugin_context()).with_probe_window(Duration::ZERO);
    assert_eq!(
        host.process("w1:p3").expect("window"),
        Some(PaneProcess::Unknown)
    );
    assert!(runner.requests().is_empty());
}

#[test]
fn every_unleased_non_agent_pane_is_probed_within_the_window() {
    let panes: Vec<_> = (0..20)
        .map(|index| json!({ "pane_id": format!("w1:p{index}"), "tab_id": "w1:t1" }))
        .collect();
    let mut responses = vec![ok(&json!({ "type": "pane_list", "panes": panes }))];
    responses.extend((0..19).map(|_| info(5, 5, &[(5, &["-zsh"])])));
    responses.push(info(9, 11, &[(11, &["proqi", "--resume", super::SESSION])]));
    let runner = ScriptedRunner::with(responses);
    let panes = host(&runner, &plugin_context())
        .tab_panes("w1:t1")
        .expect("snapshot");
    assert_eq!(runner.requests().len(), 21);
    assert!(
        panes[19].proqi_presence,
        "a late lease-free Proqi is still found"
    );
}
