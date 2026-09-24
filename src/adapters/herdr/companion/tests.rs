//! Herdr CLI translation for the companion host and its plugin state records.

use std::{cell::RefCell, collections::VecDeque, ffi::OsString, path::PathBuf, rc::Rc};

use serde_json::{Value, json};

use crate::ports::{
    companion::{CompanionError, CompanionHost, CompanionRecord, CompanionRecords, PaneProcess},
    environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner},
};

use super::{FileCompanionRecords, HerdrCompanionHost, HerdrPluginEnvironment};

const SESSION: &str = "ses_06g30t7dv5qv55n1ppn3clis3k";

#[derive(Clone, Default)]
struct ScriptedRunner {
    responses: Rc<RefCell<VecDeque<ProcessOutput>>>,
    requests: Rc<RefCell<Vec<Vec<String>>>>,
}

impl ScriptedRunner {
    fn with(responses: Vec<ProcessOutput>) -> Self {
        Self {
            responses: Rc::new(RefCell::new(responses.into())),
            requests: Rc::default(),
        }
    }

    fn requests(&self) -> Vec<Vec<String>> {
        self.requests.borrow().clone()
    }
}

impl ProcessRunner for ScriptedRunner {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        assert_eq!(request.program, OsString::from("/bin/herdr"));
        assert!(request.stdin.is_none());
        self.requests.borrow_mut().push(
            request
                .args
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect(),
        );
        self.responses
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| ProcessError::Io("unexpected Herdr call".to_owned()))
    }
}

fn ok(result: &Value) -> ProcessOutput {
    ProcessOutput {
        exit_code: Some(0),
        stdout: serde_json::to_vec(&json!({ "id": "cli", "result": result })).expect("json"),
        stderr: Vec::new(),
    }
}

fn rejected(code: &str) -> ProcessOutput {
    ProcessOutput {
        exit_code: Some(1),
        stdout: Vec::new(),
        stderr: serde_json::to_vec(&json!({ "error": { "code": code, "message": code } }))
            .expect("json"),
    }
}

fn plugin_context() -> Value {
    json!({
        "workspace_id": "w1", "workspace_label": "demo", "workspace_cwd": "/workspace",
        "tab_id": "w1:t1", "tab_label": "1", "focused_pane_id": "w1:p1",
        "focused_pane_cwd": "/work", "invocation_source": "keybinding"
    })
}

fn host(runner: &ScriptedRunner, context: &Value) -> HerdrCompanionHost<ScriptedRunner> {
    HerdrCompanionHost::new(
        HerdrPluginEnvironment::new(
            OsString::from("/bin/herdr"),
            "proqi".to_owned(),
            context.to_string(),
            PathBuf::from("/state"),
        ),
        runner.clone(),
    )
}

#[test]
fn context_reads_the_plugin_invocation_and_falls_back_to_the_workspace_directory() {
    let runner = ScriptedRunner::default();
    let context = host(&runner, &plugin_context()).context().expect("context");
    assert_eq!(context.tab_label.as_deref(), Some("1"));
    assert_eq!(context.focused_pane_cwd, PathBuf::from("/work"));
    let mut without_cwd = plugin_context();
    without_cwd["focused_pane_cwd"] = Value::Null;
    let context = host(&runner, &without_cwd).context().expect("fallback");
    assert_eq!(context.focused_pane_cwd, PathBuf::from("/workspace"));
    let malformed = host(&runner, &json!({ "tab_id": "w1:t1" })).context();
    assert!(matches!(malformed, Err(CompanionError::Unavailable(_))));
}

#[test]
fn tab_panes_keep_only_the_invoking_tab_and_detect_the_proqi_lease() {
    let runner = ScriptedRunner::with(vec![ok(&json!({ "type": "pane_list", "panes": [
        { "pane_id": "w1:p1", "tab_id": "w1:t1", "focused": true, "agent": "claude", "cwd": "/work" },
        { "pane_id": "w1:p3", "tab_id": "w1:t1", "display_agent": "proqi", "label": "Proqi", "future": 1 },
        { "pane_id": "w1:p4", "tab_id": "w1:t2", "display_agent": "proqi" }
    ]}))]);
    let panes = host(&runner, &plugin_context())
        .tab_panes("w1:t1")
        .expect("panes");
    assert_eq!(panes.len(), 2);
    assert!(panes[0].agent && panes[0].focused && !panes[0].proqi_presence);
    assert!(panes[1].proqi_presence && !panes[1].agent);
    assert_eq!(panes[1].label.as_deref(), Some("Proqi"));
    assert_eq!(runner.requests(), vec![vec!["pane", "list"]]);
}

#[test]
fn a_proqi_without_a_display_lease_is_recognized_by_its_process_only() {
    let runner = ScriptedRunner::with(vec![
        ok(&json!({ "type": "pane_list", "panes": [
            { "pane_id": "w1:p1", "tab_id": "w1:t1", "agent": "codex" },
            { "pane_id": "w1:p4", "tab_id": "w1:t1" },
            { "pane_id": "w1:p5", "tab_id": "w1:t1" },
            { "pane_id": "w1:p6", "tab_id": "w1:t1" }
        ]})),
        info(9, 11, &[(11, &["proqi", "--resume", SESSION])]),
        info(5, 5, &[(5, &["-zsh"])]),
        rejected("pane_not_found"),
    ]);
    let panes = host(&runner, &plugin_context())
        .tab_panes("w1:t1")
        .expect("panes");
    let present: Vec<_> = panes.iter().map(|pane| pane.proqi_presence).collect();
    assert_eq!(present, vec![false, true, false, false]);
    let queried: Vec<_> = runner
        .requests()
        .into_iter()
        .skip(1)
        .map(|request| request[3].clone())
        .collect();
    assert_eq!(
        queried,
        vec!["w1:p4", "w1:p5", "w1:p6"],
        "agents are never probed"
    );
}

fn info(shell: u32, group: u32, processes: &[(u32, &[&str])]) -> ProcessOutput {
    let processes: Vec<_> = processes
        .iter()
        .map(|(pid, argv)| json!({ "pid": pid, "argv": argv }))
        .collect();
    ok(&json!({ "process_info": {
        "shell_pid": shell, "foreground_process_group_id": group,
        "foreground_processes": processes
    }}))
}

#[test]
fn process_classification_distinguishes_proqi_launcher_idle_shell_and_other_work() {
    let session = SESSION.parse().expect("session");
    let cases: Vec<(ProcessOutput, Option<PaneProcess>)> = vec![
        (
            info(
                7,
                7,
                &[(7, &["/home/u/.local/bin/proqi", "--resume", SESSION])],
            ),
            Some(PaneProcess::Proqi {
                session_id: Some(session),
            }),
        ),
        (
            info(6, 9, &[(9, &["proqi", "-r", SESSION])]),
            Some(PaneProcess::Proqi {
                session_id: Some(session),
            }),
        ),
        (
            info(7, 7, &[(7, &["proqi", &format!("--resume={SESSION}")])]),
            Some(PaneProcess::Proqi {
                session_id: Some(session),
            }),
        ),
        (
            info(7, 7, &[(7, &["proqi", "--resume", "session-name"])]),
            Some(PaneProcess::Proqi { session_id: None }),
        ),
        (
            info(
                7,
                7,
                &[(7, &["sh", "/plugins/proqi/herdr-plugin/proqi.sh", "board"])],
            ),
            Some(PaneProcess::Launcher),
        ),
        (
            info(
                7,
                7,
                &[(
                    7,
                    &[
                        "sh",
                        "-c",
                        "exec sh \"/plugins/proqi/herdr-plugin/proqi.sh\" board",
                    ],
                )],
            ),
            Some(PaneProcess::Launcher),
        ),
        (info(5, 5, &[(5, &["-zsh"])]), Some(PaneProcess::IdleShell)),
        (
            info(5, 5, &[(5, &["/bin/bash"])]),
            Some(PaneProcess::IdleShell),
        ),
        (
            info(5, 8, &[(8, &["vim", "notes"])]),
            Some(PaneProcess::Other),
        ),
        (
            info(5, 5, &[(5, &["zsh", "-c", "sleep 9"])]),
            Some(PaneProcess::Other),
        ),
        (info(5, 5, &[(5, &["python3"])]), Some(PaneProcess::Other)),
        (rejected("pane_not_found"), None),
    ];
    for (response, expected) in cases {
        let runner = ScriptedRunner::with(vec![response]);
        let process = host(&runner, &plugin_context())
            .process("w1:p3")
            .expect("process");
        assert_eq!(process, expected);
        assert_eq!(
            runner.requests(),
            vec![vec!["pane", "process-info", "--pane", "w1:p3"]]
        );
    }
}

#[test]
fn open_beside_uses_the_manifest_entrypoint_without_a_shell_command() {
    let runner = ScriptedRunner::with(vec![ok(&json!({ "type": "plugin_pane_opened",
        "plugin_pane": { "entrypoint": "board", "plugin_id": "proqi",
            "pane": { "pane_id": "w1:p7", "tab_id": "w1:t1" } } }))]);
    let pane = host(&runner, &plugin_context())
        .open_beside(
            "w1:p1",
            std::path::Path::new("/work dir/ü"),
            SESSION.parse().expect("id"),
        )
        .expect("open");
    assert_eq!(pane, "w1:p7");
    let session = format!("PROQI_HERDR_SESSION={SESSION}");
    assert_eq!(
        runner.requests(),
        vec![vec![
            "plugin",
            "pane",
            "open",
            "--plugin",
            "proqi",
            "--entrypoint",
            "board",
            "--placement",
            "split",
            "--target-pane",
            "w1:p1",
            "--direction",
            "right",
            "--cwd",
            "/work dir/ü",
            "--env",
            &session,
            "--focus",
        ]]
    );
}

#[test]
fn focus_falls_back_from_plugin_ownership_to_agent_focus_to_a_zoom_cycle() {
    let runner = ScriptedRunner::with(vec![rejected("plugin_pane_not_found"), ok(&json!({}))]);
    host(&runner, &plugin_context())
        .focus("w1:p1")
        .expect("agent focus");
    assert_eq!(runner.requests()[1], vec!["agent", "focus", "w1:p1"]);

    let runner = ScriptedRunner::with(vec![
        rejected("plugin_pane_not_found"),
        rejected("agent_not_found"),
        ok(&json!({})),
        ok(&json!({})),
    ]);
    host(&runner, &plugin_context())
        .focus("w1:p5")
        .expect("zoom focus");
    assert_eq!(
        runner.requests()[2..],
        [
            vec!["pane", "zoom", "w1:p5", "--on"],
            vec!["pane", "zoom", "w1:p5", "--off"]
        ]
    );
}

#[test]
fn close_tolerates_an_already_closed_pane_and_reports_other_rejections() {
    let runner = ScriptedRunner::with(vec![rejected("pane_not_found"), rejected("busy")]);
    let mut host = host(&runner, &plugin_context());
    host.close("w1:p3").expect("already closed");
    assert!(matches!(host.close("w1:p3"), Err(CompanionError::Host(_))));
}

#[test]
fn notification_is_best_effort_and_carries_only_the_message() {
    let runner = ScriptedRunner::with(vec![rejected("disabled")]);
    host(&runner, &plugin_context()).notify("Proqi is busy");
    assert_eq!(
        runner.requests(),
        vec![vec![
            "notification",
            "show",
            "Proqi",
            "--body",
            "Proqi is busy"
        ]]
    );
}

fn record(tab: &str, pane: &str) -> CompanionRecord {
    CompanionRecord {
        tab_id: tab.to_owned(),
        pane_id: pane.to_owned(),
        session_id: SESSION.parse().expect("session"),
    }
}

#[test]
fn records_round_trip_replace_per_tab_and_survive_reopening() {
    let directory = tempfile::tempdir().expect("state");
    {
        let mut records = FileCompanionRecords::acquire(directory.path()).expect("lock");
        assert!(records.all().expect("empty").is_empty());
        records.save(&record("w1:t1", "w1:p2")).expect("save");
        records.save(&record("w2:t1", "w2:p2")).expect("save");
        records.save(&record("w1:t1", "w1:p9")).expect("replace");
        records.remove("w2:t1").expect("remove");
        records.remove("missing").expect("idempotent remove");
    }
    let mut records = FileCompanionRecords::acquire(directory.path()).expect("relock");
    assert_eq!(
        records.all().expect("records"),
        vec![record("w1:t1", "w1:p9")]
    );
    assert_eq!(
        records.load("w1:t1").expect("load"),
        Some(record("w1:t1", "w1:p9"))
    );
}

#[test]
fn unreadable_or_foreign_state_is_treated_as_no_record() {
    for contents in [
        "{",
        "{\"version\":2,\"companions\":[]}",
        "{\"version\":1,\"companions\":[],\"x\":1}",
    ] {
        let directory = tempfile::tempdir().expect("state");
        std::fs::write(directory.path().join("companions.json"), contents).expect("seed");
        let mut records = FileCompanionRecords::acquire(directory.path()).expect("lock");
        assert!(records.all().expect("records").is_empty(), "{contents}");
        records.save(&record("w1:t1", "w1:p2")).expect("overwrite");
        assert_eq!(records.all().expect("records").len(), 1);
    }
}

#[test]
fn a_second_toggle_waits_for_the_lock_and_then_reports_contention() {
    let directory = tempfile::tempdir().expect("state");
    let _held = FileCompanionRecords::acquire(directory.path()).expect("first");
    let contended = FileCompanionRecords::acquire_within(
        directory.path(),
        std::time::Duration::from_millis(60),
    );
    assert!(matches!(contended, Err(CompanionError::State(_))));
}
