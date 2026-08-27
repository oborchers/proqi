//! Mouse parity for focused-thought actions and narrow chrome.

use proqi::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, Effect, FailureCode, InteractionMode},
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::environment::IdGenerator,
    ui::{BoardApp, HitTarget, PointerButton, PointerInput, PointerKind, UiInput},
};
use ratatui_core::layout::{Rect, Size};

struct Fixture {
    app: BoardApp,
    ids: FakeIdGenerator,
    clock: FakeClock,
}

impl Fixture {
    fn new() -> Self {
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
            app: BoardApp::new(
                AppState::new(board),
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
