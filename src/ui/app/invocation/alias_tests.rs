use std::path::PathBuf;

use ratatui_core::{backend::TestBackend, terminal::Terminal};

use crate::{
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND},
        invocation::{
            InvocationEntry, InvocationForm, InvocationHarness, InvocationKind, InvocationScope,
        },
    },
    ui::{Theme, ThemePreference, render},
};

use super::contract::{app, install, target};

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

#[test]
fn manual_picker_compactly_distinguishes_shared_skill_forms() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("text", cwd.path());
    install(&mut app, cwd.path(), vec![shared_entry()]);
    app.open_invocation_picker();

    assert_eq!(
        app.invocation_view().expect("manual picker").1,
        vec!["/shared  Global Skill", "$shared  Global Skill"]
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
        app.invocation_view().expect("manual picker").1,
        vec!["/shared  Global Skill", "$shared  Global Skill"]
    );
}

#[test]
fn typed_sigils_and_adjacent_targets_each_select_one_shared_form() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for (content, target_kind, expected) in [
        ("$sh", None, "$shared  Global Skill"),
        ("/sh", None, "/shared  Global Skill"),
        ("text", Some(CODEX_AGENT_KIND), "$shared  Global Skill"),
        ("text", Some(CLAUDE_AGENT_KIND), "/shared  Global Skill"),
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
            app.invocation_view().expect("shared form").1,
            vec![expected]
        );
    }
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
