use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, ReleaseHighlightPresentation, UpdateIntent},
    domain::{
        InstallationKind, ReleaseHighlightAnnouncement, ReleaseHighlightsManifest, Session,
        SessionBoard, StableVersion, Timestamp,
    },
    ports::environment::IdGenerator as _,
    ui::{
        PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey, render,
        render_with_outcome,
    },
};
use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

use super::super::BoardApp;

fn app() -> (BoardApp, FakeIdGenerator, FakeClock) {
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    (
        BoardApp::new(AppState::new(board), RopeEditorFactory),
        ids,
        FakeClock::new(Timestamp::from_millis(2)),
    )
}

fn manifest() -> ReleaseHighlightsManifest {
    ReleaseHighlightsManifest::parse_json(
        r#"{"schema_version":1,"releases":[{"version":"1.0.0","highlights":["Fast capture","Quiet focus","Durable boards"]},{"version":"1.1.0","highlights":["Skipped releases stay grouped for context","Wide characters such as 界 and combining text stay intact","Mouse and keyboard dismissal are equivalent"]},{"version":"1.2.0","highlights":["One responsive overlay wraps concise highlights in narrow panes","Scrolling preserves the active row across shallow panes","Explicit dismissal is durable while a crash before dismissal reopens it"]}]}"#,
    )
    .expect("manifest")
}

fn install_automatic(app: &mut BoardApp, input_boundary: u64) -> ReleaseHighlightAnnouncement {
    let manifest = manifest();
    let announcement = ReleaseHighlightAnnouncement::pending(
        app.state.board.session.id,
        StableVersion::parse("1.0.0").expect("previous"),
        StableVersion::parse("1.2.0").expect("target"),
    )
    .expect("announcement");
    app.install_release_highlights(
        manifest.installed(&StableVersion::parse("1.2.0").expect("installed")),
        Some(ReleaseHighlightPresentation {
            groups: manifest
                .between(
                    &StableVersion::parse("1.0.0").expect("previous"),
                    &StableVersion::parse("1.2.0").expect("target"),
                )
                .expect("groups"),
            announcement: announcement.clone(),
        }),
        input_boundary,
    );
    announcement
}

#[test]
fn automatic_dismissal_waits_for_durable_exact_acknowledgement() {
    let (mut app, mut ids, clock) = app();
    let announcement = install_automatic(&mut app, 0);
    app.arm_release_highlights(0);

    assert_eq!(
        app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock),
        vec![Effect::Update(UpdateIntent::AcknowledgeReleaseHighlights(
            announcement
        ))]
    );
    assert!(app.release_highlights.is_some());
    app.complete_release_highlights_acknowledgement(false);
    assert!(app.release_highlights.is_some());
    app.complete_release_highlights_acknowledgement(true);
    assert!(app.release_highlights.is_none());
}

#[test]
fn protected_automatic_overlay_rejects_input_queued_before_its_first_draw() {
    let (mut app, _, _) = app();
    install_automatic(&mut app, 7);
    assert!(!app.accept_release_highlights_input(7));
    assert!(!app.accept_release_highlights_input(8));
    app.arm_release_highlights(8);
    assert!(!app.accept_release_highlights_input(7));
    assert!(!app.accept_release_highlights_input(8));
    assert!(app.accept_release_highlights_input(9));
}

#[test]
fn hidden_highlights_arm_only_when_they_become_the_rendered_overlay() {
    let (mut app, mut ids, clock) = app();
    install_automatic(&mut app, 7);
    app.present_update(
        StableVersion::parse("1.3.0").expect("update version"),
        InstallationKind::HomebrewFormula,
        2,
    );
    let mut terminal = Terminal::new(TestBackend::new(72, 16)).expect("terminal");
    let mut highlights_visible = false;
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            highlights_visible = render_with_outcome(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw update prompt");
    assert!(!highlights_visible);
    assert!(app.accept_protected_overlay_input(8));

    let effects = if app.accept_protected_overlay_input(8) {
        app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock)
    } else {
        Vec::new()
    };
    assert_eq!(effects.len(), 1);
    assert!(!app.accept_protected_overlay_input(8));
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            highlights_visible = render_with_outcome(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw revealed highlights");
    assert!(highlights_visible);
    app.arm_release_highlights(8);
    assert!(!app.accept_protected_overlay_input(8));
    assert!(app.accept_protected_overlay_input(9));
}

#[test]
fn manual_reopen_closes_without_an_update_effect() {
    let (mut app, mut ids, clock) = app();
    let manifest = manifest();
    app.install_release_highlights(
        manifest.installed(&StableVersion::parse("1.2.0").expect("installed")),
        None,
        0,
    );
    assert!(app.open_installed_release_highlights().is_empty());
    assert!(
        app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock)
            .is_empty()
    );
    assert!(app.release_highlights.is_none());
}

#[test]
fn mouse_close_is_explicit_dismissal_and_scroll_survives_resize() {
    let (mut app, mut ids, clock) = app();
    let announcement = install_automatic(&mut app, 0);
    app.arm_release_highlights(0);
    let layout = app.prepare_frame(Rect::new(0, 0, 38, 8));
    let close = layout.overlay.expect("overlay").close;
    let _ = app.handle(
        UiInput::Pointer(PointerInput {
            column: 1,
            row: 3,
            kind: PointerKind::ScrollDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    let effects = app.handle(
        UiInput::Pointer(PointerInput {
            column: close.x,
            row: close.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(
        effects,
        vec![Effect::Update(UpdateIntent::AcknowledgeReleaseHighlights(
            announcement
        ))]
    );
}

#[test]
fn scroll_and_resize_reproject_and_clamp_without_losing_the_overlay() {
    let (mut app, mut ids, clock) = app();
    install_automatic(&mut app, 0);
    app.arm_release_highlights(0);
    app.prepare_frame(Rect::new(0, 0, 38, 8));
    for _ in 0..20 {
        let _ = app.handle(
            UiInput::Key(UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        );
    }
    let shallow = app.release_highlights_view(36, 6).expect("shallow overlay");
    assert!(shallow.scroll > 0);
    let _ = app.handle(
        UiInput::Resize {
            width: 100,
            height: 24,
        },
        &mut ids,
        &clock,
    );
    app.prepare_frame(Rect::new(0, 0, 100, 24));
    let resized = app
        .release_highlights_view(56, 20)
        .expect("resized overlay");
    assert!(resized.scroll <= resized.rows.len().saturating_sub(20));
    assert_eq!(resized.title, " what's new in Proqi 1.2.0 ");
}

#[test]
fn overlay_navigation_uses_the_canonical_arrow_and_vim_modifier_parity() {
    use crate::ports::editor::CursorMovement;

    let (mut app, mut ids, clock) = app();
    install_automatic(&mut app, 0);
    app.arm_release_highlights(0);
    app.prepare_frame(Rect::new(0, 0, 38, 8));
    let inputs = [
        UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: true,
        },
        UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualDown,
        },
        UiKey::EditNavigation {
            editor_movement: CursorMovement::VisualJumpDown,
            board_movement: CursorMovement::VisualDown,
        },
        UiKey::Character('J'),
        UiKey::PrimaryCharacter('j'),
    ];
    for (index, key) in inputs.into_iter().enumerate() {
        let _ = app.handle(UiInput::Key(key), &mut ids, &clock);
        assert_eq!(
            app.release_highlights_view(36, 6).expect("overlay").scroll,
            index + 1
        );
    }
}

fn snapshot(width: u16, height: u16, preference: ThemePreference) -> String {
    let (mut app, _, _) = app();
    install_automatic(&mut app, 0);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(preference, preference != ThemePreference::Limited),
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
fn release_highlights_have_a_complete_dark_wide_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("release_highlights_dark_wide", snapshot(84, 18, ThemePreference::Dark));
    });
}

#[test]
fn release_highlights_have_a_complete_light_narrow_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("release_highlights_light_narrow", snapshot(38, 16, ThemePreference::Light));
    });
}

#[test]
fn release_highlights_have_a_complete_limited_shallow_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("release_highlights_limited_shallow", snapshot(64, 8, ThemePreference::Limited));
    });
}
