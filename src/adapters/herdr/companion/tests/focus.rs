//! Focusing a pane by identity without disturbing the tab's zoom state.

use serde_json::json;

use crate::ports::companion::CompanionHost;

use super::{ScriptedRunner, host, ok, plugin_context, rejected};

fn layout(zoomed: bool, focused: &str) -> crate::ports::environment::ProcessOutput {
    ok(&json!({ "type": "pane_layout", "layout": {
        "workspace_id": "w1", "tab_id": "w1:t1", "zoomed": zoomed,
        "focused_pane_id": focused, "panes": [], "splits": []
    }}))
}

#[test]
fn a_plugin_owned_pane_is_focused_directly() {
    let runner = ScriptedRunner::with(vec![ok(&json!({}))]);
    host(&runner, &plugin_context())
        .focus("w1:p3")
        .expect("owned focus");
    assert_eq!(
        runner.requests(),
        vec![vec!["plugin", "pane", "focus", "w1:p3"]]
    );
}

#[test]
fn an_unzoomed_tab_is_zoomed_and_unzoomed_to_focus_and_stays_unzoomed() {
    let runner = ScriptedRunner::with(vec![
        rejected("plugin_pane_not_found"),
        layout(false, "w1:p1"),
        ok(&json!({})),
        ok(&json!({})),
    ]);
    host(&runner, &plugin_context())
        .focus("w1:p5")
        .expect("focus");
    assert_eq!(
        runner.requests()[1..],
        [
            vec!["pane", "layout", "--pane", "w1:p5"],
            vec!["pane", "zoom", "w1:p5", "--on"],
            vec!["pane", "zoom", "w1:p5", "--off"],
        ]
    );
}

#[test]
fn a_zoomed_tab_keeps_its_zoom_moved_to_the_focused_target() {
    let runner = ScriptedRunner::with(vec![
        rejected("plugin_pane_not_found"),
        layout(true, "w1:p1"),
        ok(&json!({})),
        ok(&json!({})),
    ]);
    host(&runner, &plugin_context())
        .focus("w1:p5")
        .expect("focus");
    assert_eq!(
        runner.requests()[1..],
        [
            vec!["pane", "layout", "--pane", "w1:p5"],
            vec!["pane", "zoom", "w1:p1", "--off"],
            vec!["pane", "zoom", "w1:p5", "--on"],
        ]
    );
}

#[test]
fn an_already_focused_pane_changes_nothing() {
    let runner = ScriptedRunner::with(vec![
        rejected("plugin_pane_not_found"),
        layout(true, "w1:p5"),
    ]);
    host(&runner, &plugin_context())
        .focus("w1:p5")
        .expect("focus");
    assert_eq!(runner.requests().len(), 2);
}
