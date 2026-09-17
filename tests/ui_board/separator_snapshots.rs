use super::{snapshot_support::snapshot_buffer, *};
use proqi::{
    adapters::editor::RopeEditorFactory,
    application::{AppState, InteractionMode},
    domain::{Separator, Thought, ThoughtName, ThoughtPosition},
};

fn separator_fixture(pattern: &str) -> Fixture {
    let mut ids = FakeIdGenerator::new(1_726_100_000_000);
    let now = Timestamp::from_millis(10);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-separator-snapshots"),
        now,
    )
    .expect("session");
    let mut thoughts = Vec::new();
    let mut separators = Vec::new();
    for (index, kind) in pattern.chars().enumerate() {
        let position = ThoughtPosition::new(u32::try_from(index).expect("fixture position"));
        match kind {
            't' => thoughts.push(Thought::new(
                ids.thought_id(),
                session.id,
                format!("Thought {} keeps its exact text.", index + 1),
                position,
                now,
            )),
            's' => separators.push(Separator::new(
                ids.separator_id(),
                session.id,
                position,
                now,
            )),
            _ => panic!("fixture pattern"),
        }
    }
    let first = separators.first().expect("separator").id;
    let mut state =
        AppState::new(SessionBoard::with_separators(session, thoughts, separators).expect("board"));
    state.mode = InteractionMode::Board;
    state.focused_item = Some(first.into());
    Fixture {
        app: BoardApp::new(state, RopeEditorFactory),
        ids,
        clock: FakeClock::new(Timestamp::from_millis(20)),
    }
}

fn snapshot(fixture: &mut Fixture, width: u16, height: u16, theme: ThemePreference) -> String {
    snapshot_buffer(draw_theme(fixture, width, height, theme).backend().buffer())
}

#[test]
fn spacious_dark_separator_owns_the_boundary_and_hover_geometry() {
    let mut fixture = separator_fixture("tst");
    let initial = draw_theme(&mut fixture, 62, 14, ThemePreference::Dark);
    let separator = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 62, 14))
        .separators[0]
        .area;
    fixture.pointer(
        separator.x.saturating_add(3),
        separator.y,
        PointerKind::Move,
    );
    assert!(
        fixture
            .app
            .state
            .board
            .live_items()
            .into_iter()
            .all(|item| !fixture.app.item_selected(item.id()))
    );
    drop(initial);
    insta::assert_snapshot!(snapshot(&mut fixture, 62, 14, ThemePreference::Dark));
}

#[test]
fn separator_drag_uses_the_same_prepared_gutter_and_destination_geometry() {
    let mut fixture = separator_fixture("tst");
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 10));
    let separator_gutter = layout.separators[0].gutter;
    let destination = layout.thoughts[1].area;
    fixture.pointer(
        separator_gutter.x,
        separator_gutter.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(
        destination.x,
        destination.y,
        PointerKind::Drag(PointerButton::Left),
    );
    insta::assert_snapshot!(snapshot(&mut fixture, 52, 10, ThemePreference::Dark));
    assert!(matches!(
        fixture
            .effects(UiInput::Pointer(PointerInput {
                column: destination.x,
                row: destination.y,
                kind: PointerKind::Up(PointerButton::Left),
                extend_selection: false,
            }))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
}

#[test]
fn shallow_light_consecutive_separators_keep_separate_selected_rows() {
    let mut fixture = separator_fixture("ssts");
    fixture.input(crate::key_input(UiKey::Character(' ')));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: true,
    }));
    let items = fixture.app.state.board.live_items();
    assert!(fixture.app.item_selected(items[0].id()));
    assert!(fixture.app.item_selected(items[1].id()));
    assert!(!fixture.app.item_selected(items[2].id()));
    insta::assert_snapshot!(snapshot(&mut fixture, 48, 8, ThemePreference::Light));
}

#[test]
fn narrow_limited_separator_board_clips_and_scrolls_without_losing_identity() {
    let mut fixture = separator_fixture("stststs");
    for _ in 0..6 {
        fixture.input(crate::key_input(UiKey::Character('j')));
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 24, 6));
    assert!(layout.viewport_offset > 0);
    assert!(!layout.separators.is_empty());
    insta::assert_snapshot!(snapshot(&mut fixture, 24, 6, ThemePreference::Limited));
}

#[test]
fn named_thoughts_and_explicit_separator_share_one_prepared_flow() {
    let mut fixture = separator_fixture("tst");
    let thought_ids = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.id)
        .collect::<Vec<_>>();
    for (thought_id, name) in thought_ids
        .iter()
        .copied()
        .zip(["Before separator", "After separator"])
    {
        fixture
            .app
            .state
            .board
            .thought_mut(thought_id)
            .expect("thought")
            .set_name(Some(ThoughtName::new(name).expect("name")));
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 12));
    let first = layout.thought(thought_ids[0]).expect("first layout");
    let second = layout.thought(thought_ids[1]).expect("second layout");
    assert!(first.separator_before.is_none());
    assert!(second.separator_before.is_none());
    assert!(first.name.expect("first name").y < first.body_area.y);
    assert!(second.name.expect("second name").y < second.body_area.y);
    insta::assert_snapshot!(snapshot(&mut fixture, 52, 12, ThemePreference::Dark));
}
