//! Mouse parity for focused-thought actions and narrow chrome.

use proqi::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, Effect, FailureCode, InteractionMode},
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::environment::IdGenerator,
    ui::{BoardApp, HitTarget, PointerButton, PointerInput, PointerKind, UiInput, UiSettings},
};
use ratatui_core::layout::{Rect, Size};

struct Fixture {
    app: BoardApp,
    ids: FakeIdGenerator,
    clock: FakeClock,
}

impl Fixture {
    fn new() -> Self {
        Self::with_settings(UiSettings::default())
    }

    fn with_settings(settings: UiSettings) -> Self {
        let mut ids = FakeIdGenerator::new(1_725_300_000_000);
        let now = Timestamp::from_millis(10);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-mouse-actions"),
            now,
        )
        .expect("session");
        let thought = Thought::new(
            ids.thought_id(),
            session.id,
            "exact mouse content".to_owned(),
            ThoughtPosition::new(0),
            now,
        );
        let board = SessionBoard::new(session, vec![thought]).expect("board");
        Self {
            app: BoardApp::with_settings(
                AppState::new(board),
                settings,
                proqi::adapters::editor::RopeEditorFactory,
            ),
            ids,
            clock: FakeClock::new(now),
        }
    }

    fn click(&mut self, target: HitTarget, size: Size) -> Vec<Effect> {
        let layout = self
            .app
            .prepare_frame(Rect::new(0, 0, size.width, size.height));
        let area = layout
            .controls
            .iter()
            .find_map(|(candidate, area)| (*candidate == target).then_some(*area))
            .unwrap_or_else(|| panic!("visible mouse control {target:?}: {:?}", layout.controls));
        self.app.handle(
            UiInput::Pointer(PointerInput {
                column: area.right().saturating_sub(1),
                row: area.y,
                kind: PointerKind::Down(PointerButton::Left),
                extend_selection: false,
            }),
            &mut self.ids,
            &self.clock,
        )
    }
}

#[test]
fn visible_session_id_mouse_target_matches_palette_copy_intent() {
    let mut fixture = Fixture::with_settings(UiSettings {
        show_session_id: true,
        ..UiSettings::default()
    });
    fixture.app.state.board.session.name = Some("Mouse selection QA".to_owned());
    let session_id = fixture.app.state.board.session.id.to_string();
    let effects = fixture.click(HitTarget::CopySessionId, Size::new(80, 8));
    let [
        Effect::WriteClipboard {
            request_id,
            thought_id: None,
            intent: proqi::application::ClipboardIntent::CopySessionId,
            content,
        },
    ] = effects.as_slice()
    else {
        panic!("expected session ID clipboard effect");
    };
    assert_eq!(content, &session_id);
    fixture
        .app
        .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert_eq!(fixture.app.status_text(), Some("copied session ID"));
}

#[test]
fn wide_footer_exposes_copy_cut_and_delete_without_keyboard_input() {
    let mut fixture = Fixture::new();
    let size = Size::new(80, 8);
    let copy = fixture.click(HitTarget::Copy, size);
    assert!(
        matches!(copy.as_slice(), [Effect::WriteClipboard { content, .. }] if content == "exact mouse content")
    );

    let cut = fixture.click(HitTarget::Cut, size);
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("expected clipboard write");
    };
    fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let deletion = fixture.click(HitTarget::Delete, size);
    assert!(matches!(
        deletion.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn narrow_footer_keeps_the_mouse_operable_command_palette() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .click(HitTarget::Commands, Size::new(12, 3))
            .is_empty()
    );
    assert!(fixture.app.palette_view().is_some());
}

#[test]
fn remapped_wide_key_label_and_mouse_target_share_the_same_width() {
    let mut settings = UiSettings::default();
    settings.keybindings.new = ' ';
    let mut fixture = Fixture::with_settings(settings);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 80, 8));
    let area = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::Insert).then_some(*area))
        .expect("remapped New control");
    assert!(
        area.width >= 9,
        "Space New must fit its complete hit target"
    );
    assert!(matches!(
        fixture
            .click(HitTarget::Insert, Size::new(80, 8))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
}

#[test]
fn search_control_and_result_are_mouse_operable() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .click(HitTarget::Search, Size::new(80, 8))
            .is_empty()
    );
    assert!(fixture.app.search_view().is_some());

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 80, 8));
    let result = layout
        .overlay
        .expect("search overlay")
        .items
        .first()
        .copied()
        .expect("thought result");
    fixture.app.handle(
        UiInput::Pointer(PointerInput {
            column: result.x,
            row: result.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(fixture.app.search_view().is_none());
}

#[test]
fn editor_and_recovery_controls_are_mouse_operable() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .app
            .handle(
                UiInput::Key(proqi::ui::UiKey::Enter),
                &mut fixture.ids,
                &fixture.clock,
            )
            .is_empty()
    );
    assert!(matches!(
        fixture.app.state.mode,
        InteractionMode::Edit { .. }
    ));
    assert!(
        fixture
            .click(HitTarget::ExitEdit, Size::new(80, 9))
            .is_empty()
    );
    assert_eq!(fixture.app.state.mode, InteractionMode::Board);

    let effects = fixture.app.handle(
        UiInput::Paste("unsaved mouse content".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );
    let sequence = effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("pending persistence sequence");
    fixture.app.acknowledge_persistence(sequence, false);
    assert_eq!(
        fixture.click(HitTarget::Retry, Size::new(80, 9)),
        vec![Effect::RetryPersistence { sequence }]
    );

    fixture.app.acknowledge_persistence(sequence, false);
    assert!(matches!(
        fixture
            .click(HitTarget::ExportRecovery, Size::new(80, 9))
            .as_slice(),
        [Effect::ExportRecovery { .. }]
    ));
}
