#[cfg(test)]
pub(super) mod contract {
    use std::path::{Path, PathBuf};

    use ratatui_core::{backend::TestBackend, terminal::Terminal};

    use crate::{
        adapters::{
            editor::RopeEditorFactory,
            memory::{FakeClock, FakeIdGenerator},
        },
        application::{AppState, InteractionMode},
        domain::{Direction, Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
        ports::{
            agent::{
                AgentDeliveryCapabilities, AgentSessionBinding, AgentState, AgentTarget,
                CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, HarnessKind, OPENCODE_AGENT_KIND, PaneContext,
                PaneRect,
            },
            editor::CursorMovement,
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

    pub(crate) fn app(content: &str, cwd: &Path) -> (BoardApp, FakeIdGenerator, FakeClock) {
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

    pub(crate) fn entry(
        token: &str,
        kind: InvocationKind,
        scope: InvocationScope,
    ) -> InvocationEntry {
        InvocationEntry {
            name: token[1..].to_owned(),
            description: Some("Fixture description".to_owned()),
            kind,
            scope,
            source: InvocationHarness::Codex,
            forms: vec![InvocationForm {
                harness: InvocationHarness::Codex,
                token: token.to_owned(),
                precedence: 10,
            }],
            canonical_path: PathBuf::from(format!("/fixture/{}", &token[1..])),
            precedence: 10,
        }
    }

    pub(crate) fn install(app: &mut BoardApp, cwd: &Path, entries: Vec<InvocationEntry>) {
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

    pub(super) fn target(harness: &str) -> AgentTarget {
        let source = PaneContext {
            workspace_id: "w1".to_owned(),
            tab_id: "w1:t1".to_owned(),
            pane_id: "w1:p1".to_owned(),
            rect: PaneRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
        };
        AgentTarget {
            provider: "herdr".to_owned(),
            protocol: 19,
            direction: Direction::Right,
            pane_id: "w1:p2".to_owned(),
            workspace_id: source.workspace_id.clone(),
            tab_id: source.tab_id.clone(),
            agent_kind: HarnessKind::new(harness).expect("harness"),
            agent_name: harness.to_owned(),
            agent_session: AgentSessionBinding::established("session").expect("session"),
            readiness: AgentState::Idle,
            delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
            rect: PaneRect {
                x: 20,
                y: 0,
                width: 20,
                height: 20,
            },
            source,
        }
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
        let choice = &app.invocation_view().expect("popup").1[0];
        assert_eq!(choice.token, "$plan");
        assert_eq!(choice.qualifier, "Project Skill");
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
    fn shared_starters_complete_only_at_prompt_start_for_codex_and_claude() {
        let cwd = tempfile::tempdir().expect("tempdir");
        for harness in [CODEX_AGENT_KIND, CLAUDE_AGENT_KIND] {
            for (partial, token) in [("/pl", "/plan"), ("/go", "/goal")] {
                let (mut app, mut ids, clock) = app(partial, cwd.path());
                app.complete_agent_discovery(Ok(vec![target(harness)]));
                let choices = app.invocation_view().expect("starter popup").1;
                assert_eq!(choices[0].token, token);
                assert_eq!(choices[0].qualifier, "Shared Command");
                app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
                assert_eq!(
                    app.editor_snapshot().expect("editor").content,
                    format!("{token} ")
                );
            }
        }

        for content in ["prefix /pl", " /pl", "\n/pl"] {
            let (mut app, _, _) = app(content, cwd.path());
            app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
            let mut discovered = entry("/plan", InvocationKind::Command, InvocationScope::Project);
            discovered.source = InvocationHarness::ClaudeCode;
            discovered.forms[0].harness = InvocationHarness::ClaudeCode;
            install(&mut app, cwd.path(), vec![discovered]);
            app.refresh_invocation_popup();
            assert!(
                app.invocation_view().is_none(),
                "unexpected plan popup for {content:?}"
            );
        }
    }

    #[test]
    fn discovered_shared_starter_collisions_remain_document_start_only() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let content = "/plan first\nUse /plan inline\n/goal later";

        for target_kind in [Some(CODEX_AGENT_KIND), None] {
            let (mut app, _, _) = app(content, cwd.path());
            if let Some(target_kind) = target_kind {
                app.complete_agent_discovery(Ok(vec![target(target_kind)]));
            }
            let mut plan = entry("/plan", InvocationKind::Command, InvocationScope::Project);
            plan.source = InvocationHarness::ClaudeCode;
            plan.forms[0].harness = InvocationHarness::ClaudeCode;
            let mut goal = entry("/goal", InvocationKind::Skill, InvocationScope::Project);
            goal.source = InvocationHarness::ClaudeCode;
            goal.forms[0].harness = InvocationHarness::ClaudeCode;
            install(&mut app, cwd.path(), vec![plan, goal]);

            let values = app
                .invocation_ranges(content)
                .into_iter()
                .filter_map(|range| content.get(range))
                .collect::<Vec<_>>();
            assert_eq!(values, ["/plan"]);
        }

        let (mut unsupported, _, _) = app(content, cwd.path());
        unsupported.complete_agent_discovery(Ok(vec![target(OPENCODE_AGENT_KIND)]));
        let mut plan = entry("/plan", InvocationKind::Command, InvocationScope::Project);
        plan.source = InvocationHarness::ClaudeCode;
        plan.forms[0].harness = InvocationHarness::ClaudeCode;
        install(&mut unsupported, cwd.path(), vec![plan]);
        assert!(unsupported.invocation_ranges(content).is_empty());
    }

    #[test]
    fn shared_starter_picker_requires_a_supported_target_and_byte_zero() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut supported, mut ids, clock) = app("task", cwd.path());
        supported.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
        supported.handle(
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::DocumentStart,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        );
        supported.open_invocation_picker();
        let choices = supported.invocation_view().expect("manual picker").1;
        assert_eq!(choices[0].token, "/goal");
        assert_eq!(choices[0].qualifier, "Shared Command");
        assert_eq!(choices[1].token, "/plan");
        assert_eq!(choices[1].qualifier, "Shared Command");

        let (mut unsupported, _, _) = app("/pl", cwd.path());
        unsupported.complete_agent_discovery(Ok(vec![target(OPENCODE_AGENT_KIND)]));
        unsupported.refresh_invocation_popup();
        assert!(unsupported.invocation_view().is_none());
    }

    #[test]
    fn known_targets_filter_forms_while_no_known_target_keeps_authoring_fallback() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let codex = entry("$review", InvocationKind::Skill, InvocationScope::Project);
        let mut claude = entry("/audit", InvocationKind::Command, InvocationScope::Project);
        claude.source = InvocationHarness::ClaudeCode;
        claude.forms[0].harness = InvocationHarness::ClaudeCode;

        for (target_kind, expected) in [
            (Some(CODEX_AGENT_KIND), vec![("$review", "Project Skill")]),
            (Some(CLAUDE_AGENT_KIND), vec![("/audit", "Project Command")]),
            (
                None,
                vec![("$review", "Project Skill"), ("/audit", "Project Command")],
            ),
        ] {
            let (mut app, _, _) = app("text", cwd.path());
            if let Some(target_kind) = target_kind {
                app.complete_agent_discovery(Ok(vec![target(target_kind)]));
            }
            install(&mut app, cwd.path(), vec![codex.clone(), claude.clone()]);
            app.open_invocation_picker();
            let choices = app.invocation_view().expect("manual picker").1;
            assert_eq!(choices.len(), expected.len());
            for (choice, (token, qualifier)) in choices.iter().zip(expected) {
                assert_eq!(choice.token, token);
                assert_eq!(choice.qualifier, qualifier);
            }
        }
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

    fn plan_completion_snapshot() -> String {
        let cwd = tempfile::tempdir().expect("tempdir");
        let (mut app, _, _) = app("/pl", cwd.path());
        app.complete_agent_discovery(Ok(vec![target(CODEX_AGENT_KIND)]));
        let mut terminal = Terminal::new(TestBackend::new(48, 8)).expect("terminal");
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
    fn shared_starter_popup_snapshot_is_visually_an_ordinary_command() {
        let platform = if cfg!(target_os = "macos") {
            "macos"
        } else {
            "portable"
        };
        insta::with_settings!({snapshot_suffix => platform}, {
            insta::assert_snapshot!("invocation_plan_starter", plan_completion_snapshot());
        });
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

#[path = "alias_tests.rs"]
mod alias_tests;
#[path = "reference_tests.rs"]
mod reference_tests;
