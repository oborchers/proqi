use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, UpdateIntent},
    domain::{InstallationKind, Session, SessionBoard, StableVersion, Timestamp},
    ports::environment::IdGenerator as _,
    ui::{
        FastNavigation, PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput,
        UiKey, render,
    },
};
use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

use super::BoardApp;

fn app() -> (BoardApp, FakeIdGenerator, FakeClock) {
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let mut session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    session
        .rename(Some("fixture".to_owned()))
        .expect("fixture session name");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    (
        BoardApp::new(AppState::new(board), RopeEditorFactory),
        ids,
        FakeClock::new(Timestamp::from_millis(2)),
    )
}

fn version() -> StableVersion {
    StableVersion::parse("1.2.3").expect("stable version")
}

fn update_snapshot(width: u16, height: u16) -> String {
    let (mut app, _, _) = app();
    app.present_update(version(), InstallationKind::HomebrewFormula, 12);
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
fn barrier_blocks_competing_attempts_and_expires_safely() {
    let (mut app, mut ids, _) = app();
    let operation = ids.request_id();
    assert!(app.begin_update_barrier(operation, Timestamp::from_millis(10)));
    assert!(!app.begin_update_barrier(ids.request_id(), Timestamp::from_millis(11)));
    assert!(!app.expire_update_barrier(Timestamp::from_millis(9)));
    assert!(app.expire_update_barrier(Timestamp::from_millis(10)));
    assert_eq!(app.update_barrier_operation(), None);
}

#[test]
fn restart_waits_for_confirmed_receipt_delivery() {
    let (mut app, mut ids, _) = app();
    let operation = ids.request_id();
    let installed = version();
    assert!(app.begin_update_barrier(operation, Timestamp::from_millis(10)));
    assert!(app.reserve_update_restart(operation, installed.clone()));
    assert!(!app.quit);
    assert_eq!(app.update_restart(), None);
    assert!(!app.expire_update_barrier(Timestamp::from_millis(20)));

    assert!(app.finish_update_restart_delivery(operation, true));
    assert!(app.quit);
    assert_eq!(app.update_restart(), Some(&installed));
}

#[test]
fn failed_restart_delivery_keeps_the_owner_running() {
    let (mut app, mut ids, _) = app();
    let operation = ids.request_id();
    assert!(app.begin_update_barrier(operation, Timestamp::from_millis(10)));
    assert!(app.reserve_update_restart(operation, version()));

    assert!(app.finish_update_restart_delivery(operation, false));
    assert!(!app.quit);
    assert_eq!(app.update_restart(), None);
    assert_eq!(app.update_barrier_operation(), None);
}

#[test]
fn keyboard_choices_emit_one_explicit_update_intent() {
    let (mut app, mut ids, clock) = app();
    app.present_update(version(), InstallationKind::HomebrewFormula, 3);

    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    assert_eq!(
        effects,
        vec![Effect::Update(UpdateIntent::Dismiss(version()))]
    );

    app.present_update(version(), InstallationKind::StandaloneArchive, 1);
    let effects = app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);
    assert_eq!(
        effects,
        vec![Effect::Update(UpdateIntent::Dismiss(version()))]
    );
}

#[test]
fn update_list_uses_identical_arrow_and_jk_navigation() {
    for (arrow, vim) in [
        (
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                extend_selection: true,
            },
            UiKey::PrimaryCharacter('J'),
        ),
        (
            UiKey::PrimaryShiftMove {
                movement: crate::ports::editor::CursorMovement::DocumentStart,
            },
            UiKey::Character('K'),
        ),
        (
            UiKey::EditNavigation {
                editor_movement: crate::ports::editor::CursorMovement::VisualJumpDown,
                board_movement: crate::ports::editor::CursorMovement::VisualDown,
            },
            UiKey::Character('j'),
        ),
        (
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                extend_selection: false,
            },
            UiKey::Character('k'),
        ),
    ] {
        let (mut arrow_app, mut arrow_ids, clock) = app();
        let (mut vim_app, mut vim_ids, _) = app();
        arrow_app.present_update(version(), InstallationKind::HomebrewFormula, 3);
        vim_app.present_update(version(), InstallationKind::HomebrewFormula, 3);
        arrow_app.handle(UiInput::Key(arrow), &mut arrow_ids, &clock);
        vim_app.handle(UiInput::Key(vim), &mut vim_ids, &clock);
        assert_eq!(arrow_app.update_prompt_view(), vim_app.update_prompt_view());
    }
}

#[test]
fn update_prompt_fast_navigation_clamps_across_its_short_inventory() {
    let (mut app, mut ids, clock) = app();
    app.present_update(version(), InstallationKind::HomebrewFormula, 3);
    app.handle(
        UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.update_prompt_view().expect("prompt").2, 2);
    app.handle(
        UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Previous,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.update_prompt_view().expect("prompt").2, 0);
}

#[test]
fn protected_prompt_rejects_stale_input_until_its_first_frame() {
    let (mut app, mut ids, clock) = app();
    app.present_update_protected(version(), InstallationKind::HomebrewFormula, 1, 7);
    assert!(!app.accept_update_input(7));
    assert!(!app.accept_update_input(8));

    app.arm_update_prompt();
    assert!(!app.accept_update_input(7));
    assert!(app.accept_update_input(8));
    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert_eq!(
        effects,
        vec![Effect::Update(UpdateIntent::Dismiss(version()))]
    );
}

#[test]
fn mouse_can_skip_the_offered_release() {
    let (mut app, mut ids, clock) = app();
    app.present_update(version(), InstallationKind::HomebrewFormula, 2);
    let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
    let skip = layout.overlay.expect("update overlay").items[2];

    let effects = app.handle(
        UiInput::Pointer(PointerInput {
            column: skip.x,
            row: skip.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );

    assert_eq!(effects, vec![Effect::Update(UpdateIntent::Skip(version()))]);
}

#[test]
fn update_prompt_has_a_complete_wide_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("update_prompt_wide", update_snapshot(100, 18));
    });
}

#[test]
fn update_prompt_has_a_complete_narrow_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("update_prompt_narrow", update_snapshot(44, 16));
    });
}

#[test]
fn update_prompt_has_a_complete_shallow_buffer() {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!("update_prompt_shallow", update_snapshot(72, 8));
    });
}
