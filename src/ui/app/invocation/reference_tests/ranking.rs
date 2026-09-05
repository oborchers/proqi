//! Cross-source live-reference ranking and searchable-field regressions.

use crate::{
    ports::{
        agent::{AgentState, OPENCODE_AGENT_KIND},
        invocation::{InvocationForm, InvocationHarness, InvocationKind, InvocationScope},
    },
    ui::{UiInput, UiKey},
};

use super::{app, complete_live, entry, install_catalog, open_with_live, reference, reviewer};
use crate::ui::app::invocation::tests::contract::target;

#[test]
fn exact_live_token_beats_a_weaker_installed_prefix_globally() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(OPENCODE_AGENT_KIND)]));
    let mut installed = entry(
        "$placeholder",
        InvocationKind::Agent,
        InvocationScope::Project,
    );
    installed.forms = vec![InvocationForm {
        harness: InvocationHarness::OpenCode,
        token: "@reviewer-tools".to_owned(),
        precedence: 10,
    }];
    install_catalog(&mut app, cwd.path(), vec![installed]);
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    app.handle(UiInput::Paste("@reviewer".to_owned()), &mut ids, &clock);

    let choices = app.invocation_view().expect("picker").1;
    assert_eq!(choices[0].token, "reviewer");
    assert_eq!(choices[1].token, "@reviewer-tools");
}

#[test]
fn automatic_live_lookup_retains_exact_pane_identity_compatibility() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("Ask @w2:p", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    let discovery = app.handle(UiInput::Key(UiKey::Character('9')), &mut ids, &clock);
    complete_live(&mut app, &discovery, Ok(vec![reviewer(AgentState::Idle)]));

    assert_eq!(
        app.invocation_view().expect("automatic picker").1[0].token,
        "reviewer"
    );
}

#[test]
fn live_provider_label_is_not_a_manual_search_field() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    app.handle(UiInput::Paste("Herdr".to_owned()), &mut ids, &clock);

    assert!(app.invocation_view().expect("manual picker").1.is_empty());
}

#[test]
fn manual_pane_identity_stays_below_a_genuine_token_match() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    install_catalog(
        &mut app,
        cwd.path(),
        vec![entry(
            "$work-w2:p9-tools",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
    );
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    app.handle(UiInput::Paste("w2:p9".to_owned()), &mut ids, &clock);

    let choices = app.invocation_view().expect("manual picker").1;
    assert_eq!(choices[0].token, "$work-w2:p9-tools");
    assert_eq!(choices[1].token, "reviewer");
}

#[test]
fn cross_source_interleaving_renders_the_live_heading_only_once() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(OPENCODE_AGENT_KIND)]));
    let mut installed = entry(
        "$placeholder",
        InvocationKind::Agent,
        InvocationScope::Project,
    );
    installed.forms = vec![InvocationForm {
        harness: InvocationHarness::OpenCode,
        token: "@rv-tools".to_owned(),
        precedence: 10,
    }];
    install_catalog(&mut app, cwd.path(), vec![installed]);
    open_with_live(
        &mut app,
        vec![
            reference(
                "rv",
                ("w2", Some("Product")),
                ("w2:t4", Some("Review")),
                "w2:p9",
                AgentState::Idle,
            ),
            reference(
                "review-version",
                ("w3", Some("Release")),
                ("w3:t1", Some("Version")),
                "w3:p1",
                AgentState::Idle,
            ),
        ],
    );
    app.handle(UiInput::Paste("@rv".to_owned()), &mut ids, &clock);

    let choices = app.invocation_view().expect("picker").1;
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.token.as_str())
            .collect::<Vec<_>>(),
        ["rv", "@rv-tools", "review-version"]
    );
    assert_eq!(
        choices
            .iter()
            .filter(|choice| choice.group.is_some())
            .count(),
        1
    );
}
