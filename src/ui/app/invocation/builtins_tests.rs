//! Shared Codex and Claude built-in completion contracts.

use crate::{
    application::InteractionMode,
    ports::{
        agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, OPENCODE_AGENT_KIND},
        invocation::{InvocationHarness, InvocationKind, InvocationScope},
    },
    ui::{
        PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey, render,
    },
};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

use super::tests::contract::{app, entry, install, target};

const TOKENS: [&str; 19] = [
    "/btw",
    "/clear",
    "/compact",
    "/diff",
    "/fast",
    "/goal",
    "/hooks",
    "/mcp",
    "/model",
    "/new",
    "/permissions",
    "/plan",
    "/rename",
    "/resume",
    "/review",
    "/skills",
    "/status",
    "/theme",
    "/usage",
];

#[test]
fn all_shared_commands_complete_and_insert_in_edit() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for harness in [CODEX_AGENT_KIND, CLAUDE_AGENT_KIND] {
        for token in TOKENS {
            let (mut app, mut ids, clock) = app(token, cwd.path());
            app.complete_agent_discovery(Ok(vec![target(harness)]));

            let choices = app.invocation_view().expect("built-in popup").1;
            assert_eq!(choices[0].token, token, "{harness} {token}");
            assert_eq!(choices[0].qualifier, "Shared Command");
            app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
            assert_eq!(
                app.editor_snapshot().expect("editor").content,
                format!("{token} ")
            );
        }
    }
}

#[test]
fn shared_commands_complete_from_the_empty_compose_entry_path() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for harness in [CODEX_AGENT_KIND, CLAUDE_AGENT_KIND] {
        for token in TOKENS {
            let (mut app, mut ids, clock) = app("", cwd.path());
            app.enter_provisional_compose();
            assert_eq!(app.state.mode, InteractionMode::Compose);
            app.complete_agent_discovery(Ok(vec![target(harness)]));
            for character in token.chars() {
                app.handle(UiInput::Key(UiKey::Character(character)), &mut ids, &clock);
            }

            let choices = app.invocation_view().expect("compose-path popup").1;
            assert_eq!(choices[0].token, token, "{harness} {token}");
            app.handle(UiInput::Key(UiKey::Tab), &mut ids, &clock);
            assert_eq!(
                app.editor_snapshot().expect("materialized editor").content,
                format!("{token} ")
            );
        }
    }
}

#[test]
fn shared_commands_require_byte_zero_and_a_compatible_target() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for content in [
        " /clear",
        "prefix /clear",
        "\n/clear",
        "/clearer",
        "```text\n/clear",
    ] {
        let (mut app, _, _) = app(content, cwd.path());
        app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
        app.refresh_invocation_popup();
        assert!(
            app.invocation_view().is_none(),
            "unexpected popup for {content:?}"
        );
        assert!(
            app.invocation_ranges(content).is_empty(),
            "unexpected highlight for {content:?}"
        );
    }

    for target_kind in [None, Some(OPENCODE_AGENT_KIND)] {
        let (mut app, _, _) = app("/clear", cwd.path());
        if let Some(target_kind) = target_kind {
            app.complete_agent_discovery(Ok(vec![target(target_kind)]));
        }
        app.refresh_invocation_popup();
        assert!(app.invocation_view().is_none());
        assert!(app.invocation_ranges("/clear").is_empty());
    }
}

#[test]
fn every_shared_command_highlights_only_as_an_exact_byte_zero_token() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));

    for token in TOKENS {
        let content = format!("{token} argument with Unicode Grüße 👩‍💻");
        let values = app
            .invocation_ranges(&content)
            .into_iter()
            .filter_map(|range| content.get(range))
            .collect::<Vec<_>>();
        assert_eq!(values, [token], "{token}");
    }
}

#[test]
fn discovered_collisions_stay_deduplicated_and_byte_zero_only() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for token in TOKENS {
        let (mut first_app, _, _) = app(token, cwd.path());
        first_app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
        let discovered = entry(token, InvocationKind::Command, InvocationScope::Project);
        install(&mut first_app, cwd.path(), vec![discovered.clone()]);
        first_app.refresh_invocation_popup();

        let choices = first_app.invocation_view().expect("collision popup").1;
        assert_eq!(choices.len(), 1, "{token}");
        assert_eq!(choices[0].token, token);
        assert_eq!(choices[0].qualifier, "Shared Command");

        let later = format!("prefix {token}");
        let (mut later_app, _, _) = app(&later, cwd.path());
        later_app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
        install(&mut later_app, cwd.path(), vec![discovered]);
        later_app.refresh_invocation_popup();
        assert!(later_app.invocation_view().is_none(), "{token}");
        assert!(later_app.invocation_ranges(&later).is_empty(), "{token}");
    }
}

#[test]
fn discovered_collisions_keep_byte_zero_behavior_without_a_shared_target() {
    let cwd = tempfile::tempdir().expect("tempdir");
    for (target_kind, harness) in [
        (None, InvocationHarness::Codex),
        (Some(OPENCODE_AGENT_KIND), InvocationHarness::OpenCode),
    ] {
        let token = "/review";
        let mut discovered = entry(token, InvocationKind::Command, InvocationScope::Project);
        discovered.source = harness;
        discovered.forms[0].harness = harness;

        let (mut first_app, _, _) = app(token, cwd.path());
        if let Some(target_kind) = target_kind {
            first_app.complete_agent_discovery(Ok(vec![target(target_kind)]));
        }
        install(&mut first_app, cwd.path(), vec![discovered.clone()]);
        first_app.refresh_invocation_popup();
        let choices = first_app.invocation_view().expect("discovered popup").1;
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].token, token);
        assert_eq!(choices[0].qualifier, "Project Command");
        let ranges = first_app.invocation_ranges(token);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 0..token.len());

        let later = format!("prefix {token}");
        let (mut later_app, _, _) = app(&later, cwd.path());
        if let Some(target_kind) = target_kind {
            later_app.complete_agent_discovery(Ok(vec![target(target_kind)]));
        }
        install(&mut later_app, cwd.path(), vec![discovered]);
        later_app.refresh_invocation_popup();
        assert!(later_app.invocation_view().is_none());
        assert!(later_app.invocation_ranges(&later).is_empty());
    }
}

#[test]
fn manual_picker_keeps_the_complete_stable_inventory_scrollable() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
    app.open_invocation_picker();
    let choices = app.invocation_view().expect("manual picker").1;
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.token.as_str())
            .collect::<Vec<_>>(),
        TOKENS
    );

    for _ in 1..TOKENS.len() {
        app.handle(UiInput::Key(UiKey::PickerNext), &mut ids, &clock);
    }
    let (_, visible, selected) = app.invocation_view().expect("scrolled picker");
    assert_eq!(visible[selected].token, "/usage");
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "/usage ");
}

#[test]
fn pointer_activation_inserts_a_shared_command_without_submitting() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("/cl", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
    let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 50, 12));
    let item = layout.overlay.expect("overlay").items[0];
    let effects = app.handle(
        UiInput::Pointer(PointerInput {
            column: item.x,
            row: item.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert!(effects.is_empty());
    assert_eq!(app.editor_snapshot().expect("editor").content, "/clear ");
}

#[test]
fn escape_cancels_shared_command_completion_without_mutating_text() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("/cl", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
    assert!(app.invocation_view().is_some());

    let effects = app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);

    assert!(effects.is_empty());
    assert!(app.invocation_view().is_none());
    assert_eq!(app.editor_snapshot().expect("editor").content, "/cl");
}

#[test]
fn shared_command_catalog_has_a_reviewed_shallow_snapshot() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("", cwd.path());
    app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
    app.open_invocation_picker();

    let mut terminal = Terminal::new(TestBackend::new(56, 10)).expect("terminal");
    let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 56, 10));
    let overlay = layout.overlay.as_ref().expect("overlay").area;
    terminal
        .draw(|frame| {
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let snapshot = (overlay.y..overlay.y.saturating_add(overlay.height))
        .map(|row| {
            let content = (overlay.x..overlay.x.saturating_add(overlay.width))
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{:02}│{}│", row - overlay.y, content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("invocation_shared_commands", snapshot);
}
