#[cfg(test)]
mod contract {
    use std::path::{Path, PathBuf};

    use ratatui_core::{backend::TestBackend, terminal::Terminal};

    use crate::{
        adapters::{
            editor::RopeEditorFactory,
            memory::{FakeClock, FakeIdGenerator},
        },
        application::{AppState, InteractionMode},
        domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
        ports::{
            environment::IdGenerator as _,
            invocation::{
                InvocationDiscovery, InvocationEntry, InvocationForm, InvocationHarness,
                InvocationKind, InvocationScope,
            },
        },
        ui::{
            PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey,
            render,
        },
    };

    use crate::ui::BoardApp;

    fn app(content: &str, cwd: &Path) -> (BoardApp, FakeIdGenerator, FakeClock) {
        let mut ids = FakeIdGenerator::new(1_900_000_000_000);
        let mut session = Session::new(ids.session_id(), cwd.to_owned(), Timestamp::from_millis(1))
            .expect("session");
        session
            .rename(Some("fixture".to_owned()))
            .expect("fixture name");
        let thought_id = ids.thought_id();
        let thought = Thought::new(
            thought_id,
            session.id,
            content.to_owned(),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        let mut app = BoardApp::with_settings_and_cwd(
            AppState::new(board),
            crate::ui::UiSettings::default(),
            cwd.to_owned(),
            RopeEditorFactory,
        );
        app.state.focused_thought = Some(thought_id);
        app.state.mode = InteractionMode::Edit { thought_id };
        app.sync_editor_from_state();
        (app, ids, FakeClock::new(Timestamp::from_millis(2)))
    }

    fn entry(token: &str, kind: InvocationKind, scope: InvocationScope) -> InvocationEntry {
        InvocationEntry {
            name: token[1..].to_owned(),
            description: Some("Fixture description".to_owned()),
            kind,
            scope,
            source: InvocationHarness::Codex,
            forms: vec![InvocationForm {
                harness: InvocationHarness::Codex,
                token: token.to_owned(),
            }],
            canonical_path: PathBuf::from(format!("/fixture/{}", &token[1..])),
            precedence: 10,
        }
    }

    fn install(app: &mut BoardApp, cwd: &Path, entries: Vec<InvocationEntry>) {
        let effects = app.refresh_invocations();
        let [crate::application::Effect::DiscoverInvocations(request)] = effects.as_slice() else {
            panic!("refresh effect");
        };
        app.complete_invocation_discovery(Ok(InvocationDiscovery {
            generation: request.generation,
            cwd: cwd.to_owned(),
            global: Vec::new(),
            project: entries,
        }));
    }

    #[test]
    fn completion_replaces_only_the_token_at_the_cursor_and_undoes_once() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, mut ids, clock) = app("$pl middle $pl", cwd.path());
        install(
            &mut app,
            cwd.path(),
            vec![entry(
                "$plan",
                InvocationKind::Skill,
                InvocationScope::Project,
            )],
        );

        app.refresh_invocation_popup();
        assert_eq!(
            app.invocation_view().expect("popup").1[0],
            "$plan  Project Skill"
        );
        app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
        assert_eq!(
            app.editor_snapshot().expect("editor").content,
            "$pl middle $plan "
        );

        app.handle(UiInput::Key(UiKey::Undo), &mut ids, &clock);
        assert_eq!(
            app.editor_snapshot().expect("editor").content,
            "$pl middle $pl"
        );
    }

    #[test]
    fn shell_variables_controls_and_fenced_code_do_not_open_completion() {
        let cwd = tempfile::tempdir().expect("tempdir");
        for content in [
            "$HOME",
            "$1",
            "$-",
            "$_",
            "price $5",
            "/usr/local/bin",
            "https://example.com/plan",
            "```\n$pl",
        ] {
            let (mut app, _, _) = app(content, cwd.path());
            install(
                &mut app,
                cwd.path(),
                vec![entry(
                    "$plan",
                    InvocationKind::Skill,
                    InvocationScope::Global,
                )],
            );
            app.refresh_invocation_popup();
            assert!(
                app.invocation_view().is_none(),
                "unexpected popup for {content:?}"
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

        let labels = app.invocation_view().expect("typed results").1;
        assert!(labels.iter().any(|label| label.contains("Project Skill")));
        assert!(labels.iter().any(|label| label.contains("Project Command")));
        assert!(labels.iter().all(|label| label.contains("Claude Code")));
    }

    #[test]
    fn documented_precedence_orders_global_claude_skills_before_project_skills() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, _, _) = app("/pl", cwd.path());
        let mut project = entry("/plan", InvocationKind::Skill, InvocationScope::Project);
        project.precedence = 25;
        project.canonical_path = PathBuf::from("/fixture/project-plan");
        let mut global = entry("/plan", InvocationKind::Skill, InvocationScope::Global);
        global.precedence = 5;
        global.canonical_path = PathBuf::from("/fixture/global-plan");
        let refresh = app.refresh_invocations();
        let [crate::application::Effect::DiscoverInvocations(request)] = refresh.as_slice() else {
            panic!("refresh effect");
        };
        app.complete_invocation_discovery(Ok(InvocationDiscovery {
            generation: request.generation,
            cwd: cwd.path().to_owned(),
            global: vec![global],
            project: vec![project],
        }));
        app.refresh_invocation_popup();

        let labels = app.invocation_view().expect("ordered results").1;
        assert!(labels[0].contains("Global Skill"));
        assert!(labels[1].contains("Project Skill"));
    }

    #[test]
    fn unicode_whitespace_and_long_manual_queries_remain_bounded() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, mut ids, clock) = app("Use\u{2003}$pl", cwd.path());
        install(
            &mut app,
            cwd.path(),
            vec![entry(
                "$plan",
                InvocationKind::Skill,
                InvocationScope::Project,
            )],
        );
        app.refresh_invocation_popup();
        assert!(app.invocation_view().is_some());

        app.invocation_popup = None;
        app.open_invocation_picker();
        app.handle(UiInput::Paste("界".repeat(200)), &mut ids, &clock);
        let query = app.invocation_view().expect("manual picker").0;
        assert_eq!(query.chars().count(), 128);
    }

    #[test]
    fn stale_results_cannot_replace_the_newest_generation_or_cwd() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let other = tempfile::tempdir().expect("other tempdir");
        let (mut app, _, _) = app("$pl", cwd.path());
        let first = app.refresh_invocations();
        let second = app.refresh_invocations();
        let generation = match &second[0] {
            crate::application::Effect::DiscoverInvocations(request) => request.generation,
            _ => panic!("second refresh"),
        };
        let old_generation = match &first[0] {
            crate::application::Effect::DiscoverInvocations(request) => request.generation,
            _ => panic!("first refresh"),
        };
        app.complete_invocation_discovery(Ok(InvocationDiscovery {
            generation: old_generation,
            cwd: cwd.path().to_owned(),
            global: Vec::new(),
            project: vec![entry(
                "$old",
                InvocationKind::Skill,
                InvocationScope::Project,
            )],
        }));
        app.complete_invocation_discovery(Ok(InvocationDiscovery {
            generation,
            cwd: other.path().to_owned(),
            global: Vec::new(),
            project: vec![entry(
                "$wrong",
                InvocationKind::Skill,
                InvocationScope::Project,
            )],
        }));
        app.refresh_invocation_popup();
        assert!(app.invocation_view().is_none());
    }

    #[test]
    fn mouse_click_uses_rendered_picker_geometry() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, mut ids, clock) = app("$pl", cwd.path());
        install(
            &mut app,
            cwd.path(),
            vec![entry(
                "$plan",
                InvocationKind::Skill,
                InvocationScope::Project,
            )],
        );
        app.refresh_invocation_popup();
        let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 50, 12));
        let item = layout.overlay.expect("overlay").items[0];
        app.handle(
            UiInput::Pointer(PointerInput {
                column: item.x,
                row: item.y,
                kind: PointerKind::Down(PointerButton::Left),
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        );
        assert_eq!(app.editor_snapshot().expect("editor").content, "$plan ");
    }

    fn completion_snapshot(width: u16, height: u16) -> String {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, _, _) = app("Use $pl", cwd.path());
        install(
            &mut app,
            cwd.path(),
            vec![
                entry("$plan", InvocationKind::Skill, InvocationScope::Project),
                entry("$plugin", InvocationKind::Skill, InvocationScope::Global),
            ],
        );
        app.refresh_invocation_popup();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
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
        (0..buffer.area.height)
            .map(|row| {
                let content = (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>();
                format!("{row:02}│{}│", content.trim_end_matches(' '))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn completion_popup_snapshot_covers_narrow_terminal() {
        insta::assert_snapshot!("invocation_completion_narrow", completion_snapshot(40, 12));
    }

    #[test]
    fn completion_popup_snapshot_covers_shallow_terminal() {
        insta::assert_snapshot!("invocation_completion_shallow", completion_snapshot(28, 6));
    }
}
