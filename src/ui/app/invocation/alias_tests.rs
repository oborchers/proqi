use std::path::PathBuf;

use ratatui_core::{backend::TestBackend, terminal::Terminal};

use crate::{
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
        invocation::{
            InvocationDiscovery, InvocationEntry, InvocationForm, InvocationHarness,
            InvocationKind, InvocationScope,
        },
    },
    ui::{Theme, ThemePreference, UiInput, UiKey, render},
};

use super::contract::{app, entry, install, target};

fn shared_entry() -> InvocationEntry {
    InvocationEntry {
        name: "shared".to_owned(),
        description: Some("Shared skill".to_owned()),
        kind: InvocationKind::Skill,
        scope: InvocationScope::Global,
        source: InvocationHarness::AgentSkills,
        forms: vec![
            InvocationForm {
                harness: InvocationHarness::ClaudeCode,
                token: "/shared".to_owned(),
                precedence: 5,
            },
            InvocationForm {
                harness: InvocationHarness::Codex,
                token: "$shared".to_owned(),
                precedence: 40,
            },
        ],
        canonical_path: PathBuf::from("/fixture/.agents/skills/shared/SKILL.md"),
        precedence: 5,
    }
}

fn choice_fields(app: &crate::ui::BoardApp) -> Vec<(String, String)> {
    app.invocation_view()
        .expect("invocation picker")
        .1
        .into_iter()
        .map(|choice| (choice.token, choice.qualifier))
        .collect()
}

#[test]
fn manual_picker_compactly_distinguishes_shared_skill_forms() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("text", cwd.path());
    install(&mut app, cwd.path(), vec![shared_entry()]);
    app.open_invocation_picker();

    assert_eq!(
        choice_fields(&app),
        vec![
            ("/shared".to_owned(), "Global Skill".to_owned()),
            ("$shared".to_owned(), "Global Skill".to_owned()),
        ]
    );
}

#[test]
fn manual_picker_distinguishes_independent_same_name_copies() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("text", cwd.path());
    let mut codex = shared_entry();
    codex.forms.remove(0);
    let mut claude = shared_entry();
    claude.forms.remove(1);
    claude.source = InvocationHarness::ClaudeCode;
    claude.canonical_path = PathBuf::from("/fixture/.claude/skills/shared/SKILL.md");
    install(&mut app, cwd.path(), vec![codex, claude]);
    app.open_invocation_picker();

    assert_eq!(
        choice_fields(&app),
        vec![
            ("/shared".to_owned(), "Global Skill".to_owned()),
            ("$shared".to_owned(), "Global Skill".to_owned()),
        ]
    );
}

#[test]
fn typed_sigils_and_adjacent_targets_each_select_one_shared_form() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for (content, target_kind, expected) in [
        ("$sh", None, ("$shared", "Global Skill")),
        ("/sh", None, ("/shared", "Global Skill")),
        ("text", Some(CODEX_AGENT_KIND), ("$shared", "Global Skill")),
        ("text", Some(CLAUDE_AGENT_KIND), ("/shared", "Global Skill")),
    ] {
        let (mut app, _, _) = app(content, cwd.path());
        if let Some(target_kind) = target_kind {
            app.complete_agent_discovery(Ok(vec![target(target_kind)]));
        }
        install(&mut app, cwd.path(), vec![shared_entry()]);
        if target_kind.is_some() {
            app.open_invocation_picker();
        } else {
            app.refresh_invocation_popup();
        }
        assert_eq!(
            choice_fields(&app),
            vec![(expected.0.to_owned(), expected.1.to_owned())]
        );
    }
}

#[test]
fn same_slash_token_retains_typed_disambiguation() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("/pl", cwd.path());
    let mut skill = entry("/plan", InvocationKind::Skill, InvocationScope::Project);
    skill.source = InvocationHarness::ClaudeCode;
    skill.forms[0].harness = InvocationHarness::ClaudeCode;
    let mut command = entry("/plan", InvocationKind::Command, InvocationScope::Project);
    command.source = InvocationHarness::ClaudeCode;
    command.forms[0].harness = InvocationHarness::ClaudeCode;
    install(&mut app, cwd.path(), vec![skill, command]);
    app.refresh_invocation_popup();

    let choices = app.invocation_view().expect("typed results").1;
    assert!(
        choices
            .iter()
            .any(|choice| choice.qualifier.contains("Project Skill"))
    );
    assert!(
        choices
            .iter()
            .any(|choice| choice.qualifier.contains("Project Command"))
    );
    assert!(
        choices
            .iter()
            .all(|choice| choice.qualifier.contains("Claude"))
    );
}

#[test]
fn documented_precedence_orders_global_claude_skills_before_project_skills() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("/pl", cwd.path());
    let mut project = entry("/plan", InvocationKind::Skill, InvocationScope::Project);
    project.precedence = 25;
    project.forms[0].precedence = 25;
    project.canonical_path = PathBuf::from("/fixture/project-plan");
    let mut global = entry("/plan", InvocationKind::Skill, InvocationScope::Global);
    global.precedence = 5;
    global.forms[0].precedence = 5;
    global.canonical_path = PathBuf::from("/fixture/global-plan");
    let refresh = app.refresh_invocations();
    let [crate::application::Effect::DiscoverInvocations(request)] = refresh.as_slice() else {
        panic!("refresh effect");
    };
    app.complete_invocation_discovery(InvocationDiscovery {
        generation: request.generation,
        cwd: cwd.path().to_owned(),
        global: vec![global],
        project: vec![project],
        completeness: crate::ports::invocation::InvocationCompleteness::default(),
    });
    app.refresh_invocation_popup();

    let choices = app.invocation_view().expect("ordered results").1;
    assert!(choices[0].qualifier.contains("Global Skill"));
    assert!(choices[1].qualifier.contains("Project Skill"));
}

#[test]
fn visually_truncated_token_still_inserts_fully_and_undoes_once() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let token = "$extraordinarily-long-unicode-界-skill-name";
    let (mut app, mut ids, clock) = app("$ex", cwd.path());
    install(
        &mut app,
        cwd.path(),
        vec![entry(
            token,
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
    );
    app.refresh_invocation_popup();

    let mut terminal = Terminal::new(TestBackend::new(30, 8)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui_core::buffer::Cell::symbol)
        .collect::<String>();
    assert!(rendered.contains('…'));
    assert!(!rendered.contains("Project Skill"));

    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        format!("{token} ")
    );
    app.handle(UiInput::Key(UiKey::Undo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "$ex");
}

#[test]
fn shared_skill_manual_picker_snapshot() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("text", cwd.path());
    install(&mut app, cwd.path(), vec![shared_entry()]);
    app.open_invocation_picker();
    let mut terminal = Terminal::new(TestBackend::new(48, 9)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let snapshot = (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("invocation_shared_skill_picker", snapshot);
}
