#![cfg(unix)]
//! End-to-end Herdr adapter contract through a real fake executable.

use std::{ffi::OsString, os::unix::fs::PermissionsExt as _};

use proqi::{
    adapters::{herdr::HerdrGateway, memory::FakeIdGenerator, process::SystemProcessRunner},
    ports::{
        agent::{AgentGateway, SubmissionRequest},
        environment::IdGenerator,
        invocation::InvocationReferenceCatalog,
    },
};

#[test]
fn recorded_fake_executable_proves_direct_semantic_cli_contract() {
    let temporary = tempfile::tempdir().expect("fixture directory");
    let executable = temporary.path().join("herdr-fixture");
    let prompt_log = temporary.path().join("prompt.bin");
    std::fs::write(&executable, fixture_script(&prompt_log)).expect("fixture executable");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
        .expect("fixture permissions");

    let mut gateway = HerdrGateway::new(
        OsString::from(executable.as_os_str()),
        SystemProcessRunner::default(),
        true,
    );
    let references = gateway.discover_live_references().expect("live references");
    let [reference] = references.references.as_slice() else {
        panic!("expected one live reference");
    };
    assert_eq!(reference.agent_name(), Some("fixture"));
    assert_eq!(reference.workspace_id(), "w1");
    assert_eq!(reference.workspace_label(), Some("Fixture workspace"));
    assert_eq!(reference.tab_id(), "w1:t1");
    assert_eq!(reference.tab_label(), Some("Fixture tab"));
    assert_eq!(reference.pane_id(), "w1:p2");
    let capabilities = gateway.capabilities().expect("capabilities");
    let targets = gateway
        .adjacent_targets(&capabilities.context)
        .expect("verified targets");
    let [target] = targets.as_slice() else {
        panic!("expected one verified target");
    };
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let exact = "$(touch never); Grüße\n第二行";
    let receipt = gateway
        .submit(SubmissionRequest {
            submission_id: ids.submission_id(),
            target: target.clone(),
            content: exact.to_owned(),
        })
        .expect("accepted prompt");

    assert_eq!(receipt.target, *target);
    assert_eq!(
        std::fs::read(prompt_log).expect("recorded prompt"),
        exact.as_bytes()
    );
}

fn fixture_script(prompt_log: &std::path::Path) -> String {
    format!(
        r#"#!/bin/sh
if [ "$1 $2 $3" = "api schema --json" ]; then
  printf '%s\n' '{{"protocol":19,"schema_version":1,"schemas":{{"request":{{"const":"agent.prompt"}},"response":{{"const":"agent_prompted"}}}}}}'
elif [ "$1 $2" = "api snapshot" ]; then
  printf '%s\n' '{{"result":{{"snapshot":{{"protocol":19,"version":"0.8.0","workspaces":[{{"workspace_id":"w1","label":"Fixture workspace"}}],"tabs":[{{"workspace_id":"w1","tab_id":"w1:t1","label":"Fixture tab"}}],"agents":[{{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"idle","cwd":"/private/not-a-label","terminal_title":"private prompt"}}]}}}}}}'
elif [ "$1 $2 $3" = "pane current --current" ]; then
  printf '%s\n' '{{"result":{{"pane":{{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1"}}}}}}'
elif [ "$1 $2" = "pane layout" ]; then
  printf '%s\n' '{{"result":{{"layout":{{"workspace_id":"w1","tab_id":"w1:t1","panes":[{{"pane_id":"w1:p1","rect":{{"x":0,"y":0,"width":20,"height":20}}}}]}}}}}}'
elif [ "$1 $2" = "agent list" ]; then
  printf '%s\n' '{{"result":{{"agents":[{{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"idle","agent_session":{{"agent":"codex","kind":"id","source":"herdr:codex","value":"fixture-session"}}}}]}}}}'
elif [ "$1 $2" = "pane neighbor" ]; then
  if [ "$6" = "right" ]; then
    printf '%s\n' '{{"result":{{"neighbor":{{"pane_id":"w1:p1","direction":"right","neighbor_pane_id":"w1:p2","layout":{{"workspace_id":"w1","tab_id":"w1:t1","panes":[{{"pane_id":"w1:p1","rect":{{"x":0,"y":0,"width":20,"height":20}}}},{{"pane_id":"w1:p2","rect":{{"x":20,"y":0,"width":20,"height":20}}}}]}}}}}}}}'
  else
    printf '{{"result":{{"neighbor":{{"pane_id":"w1:p1","direction":"%s","layout":{{"workspace_id":"w1","tab_id":"w1:t1","panes":[{{"pane_id":"w1:p1","rect":{{"x":0,"y":0,"width":20,"height":20}}}}]}}}}}}}}\n' "$6"
  fi
elif [ "$1 $2 $3" = "agent prompt w1:p2" ]; then
  printf '%s' "$4" > "{}"
  printf '%s\n' '{{"result":{{"type":"agent_prompted","agent":{{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t1","agent":"codex","name":"fixture","agent_status":"working","agent_session":{{"agent":"codex","kind":"id","source":"herdr:codex","value":"fixture-session"}}}}}}}}'
else
  printf '%s\n' '{{"error":{{"code":"fixture_unknown","message":"unexpected command"}}}}' >&2
  exit 1
fi
"#,
        prompt_log.display()
    )
}
