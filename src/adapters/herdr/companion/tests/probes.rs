//! Process probes: a bounded budget, and unknown results that never enable a close.

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
fn probes_beyond_the_budget_report_unknown_without_calling_herdr() {
    let responses = (0..12).map(|_| info(5, 5, &[(5, &["-zsh"])])).collect();
    let runner = ScriptedRunner::with(responses);
    let mut host = host(&runner, &plugin_context());
    for _ in 0..12 {
        assert_eq!(
            host.process("w1:p3").expect("probe"),
            Some(PaneProcess::IdleShell)
        );
    }
    assert_eq!(
        host.process("w1:p3").expect("budget"),
        Some(PaneProcess::Unknown)
    );
    assert_eq!(runner.requests().len(), 12);
}

#[test]
fn presence_probes_are_capped_so_the_recorded_pane_keeps_budget() {
    let panes: Vec<_> = (0..20)
        .map(|index| json!({ "pane_id": format!("w1:p{index}"), "tab_id": "w1:t1" }))
        .collect();
    let mut responses = vec![ok(&json!({ "type": "pane_list", "panes": panes }))];
    responses.extend((0..8).map(|_| info(5, 5, &[(5, &["-zsh"])])));
    let runner = ScriptedRunner::with(responses);
    host(&runner, &plugin_context())
        .tab_panes("w1:t1")
        .expect("snapshot");
    assert_eq!(runner.requests().len(), 9);
}
