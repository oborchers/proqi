//! Shared target fixtures for snapshots that exercise agent-aware board chrome.

use proqi::{
    application::Effect,
    domain::Direction,
    ports::{
        agent::{
            AgentSessionBinding, AgentState, AgentTarget, CODEX_AGENT_KIND, HarnessKind,
            PaneContext, PaneRect,
        },
        invocation::{
            InvocationDiscovery, InvocationEntry, InvocationForm, InvocationHarness,
            InvocationKind, InvocationScope,
        },
    },
};
use ratatui_core::{
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
    terminal::Terminal,
};

use super::Fixture;

pub(super) fn assert_invocation_styles(
    terminal: &Terminal<TestBackend>,
    area: Rect,
    annotation: Color,
) {
    let inline = &terminal.backend().buffer()[(area.x + 4, area.y + 1)];
    assert_eq!(inline.symbol(), "/");
    assert_eq!(inline.fg, annotation);
    assert!(inline.modifier.contains(Modifier::BOLD));

    let collision = &terminal.backend().buffer()[(area.x + 32, area.y + 1)];
    assert_eq!(collision.symbol(), "/");
    assert_ne!(collision.fg, annotation);
    assert!(!collision.modifier.contains(Modifier::BOLD));
}

pub(super) fn install_inline_invocation_fixture(fixture: &mut Fixture) -> &'static str {
    fixture
        .app
        .complete_agent_discovery(Ok(vec![adjacent_target(
            Direction::Right,
            "w1:p2",
            AgentState::Idle,
        )]));
    let effects = fixture.app.refresh_invocations();
    let [Effect::DiscoverInvocations(request)] = effects.as_slice() else {
        panic!("invocation refresh effect");
    };
    fixture
        .app
        .complete_invocation_discovery(Ok(InvocationDiscovery {
            generation: request.generation,
            cwd: request.cwd.clone(),
            global: vec![
                InvocationEntry {
                    name: "review".to_owned(),
                    description: Some("Review the change".to_owned()),
                    kind: InvocationKind::Skill,
                    scope: InvocationScope::Global,
                    source: InvocationHarness::AgentSkills,
                    forms: vec![
                        InvocationForm {
                            harness: InvocationHarness::Codex,
                            token: "$review".to_owned(),
                            precedence: 20,
                        },
                        InvocationForm {
                            harness: InvocationHarness::Codex,
                            token: "/implement-in-worktree".to_owned(),
                            precedence: 21,
                        },
                    ],
                    canonical_path: std::path::PathBuf::from("/fixture/review/SKILL.md"),
                    precedence: 20,
                },
                InvocationEntry {
                    name: "plan".to_owned(),
                    description: Some("Colliding discovered command".to_owned()),
                    kind: InvocationKind::Command,
                    scope: InvocationScope::Project,
                    source: InvocationHarness::ClaudeCode,
                    forms: vec![InvocationForm {
                        harness: InvocationHarness::ClaudeCode,
                        token: "/plan".to_owned(),
                        precedence: 30,
                    }],
                    canonical_path: std::path::PathBuf::from("/fixture/plan.md"),
                    precedence: 30,
                },
            ],
            project: Vec::new(),
        }));
    let content = "/plan ask $review\nUse /implement-in-worktree then /plan";
    fixture.input(super::UiInput::Paste(content.to_owned()));
    content
}

pub(super) fn adjacent_target(
    direction: Direction,
    pane_id: &str,
    readiness: AgentState,
) -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 40,
            y: 20,
            width: 40,
            height: 20,
        },
    };
    AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 19,
        direction,
        pane_id: pane_id.to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        agent_name: format!("Codex {pane_id}"),
        agent_session: AgentSessionBinding::established(format!("session-{pane_id}"))
            .expect("fixture session"),
        readiness,
        delivery: proqi::ports::agent::AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: source.rect,
        source,
    }
}
