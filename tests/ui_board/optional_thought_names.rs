//! Optional thought-name interaction, payload separation, and responsive rendering.

use super::*;

use proqi::{
    application::{DurabilityState, InteractionMode},
    domain::{BoardOperationKind, ThoughtName, ThoughtPresentation},
    ui::BoardDensity,
};
use ratatui_core::style::Modifier;

use super::snapshot_support::snapshot_buffer;

fn control_r() -> UiInput {
    UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Character('r')).with_modifiers(LogicalModifiers::CONTROL),
    )
}

fn operation_sequence(effects: &[Effect]) -> OperationSequence {
    effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("durable operation sequence")
}

fn commit_name(fixture: &mut Fixture, input: &str) -> OperationSequence {
    fixture.input(control_r());
    fixture.input(UiInput::Paste(input.to_owned()));
    let effects = fixture.effects(crate::key_input(UiKey::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if operation.kind == BoardOperationKind::Rename
    ));
    operation_sequence(&effects)
}

fn named_fixture(settings: UiSettings, content: &str, name: &str) -> Fixture {
    let mut fixture = Fixture::with_settings(settings);
    let create = fixture.paste(content);
    fixture.app.acknowledge_persistence(create, true);
    let rename = commit_name(&mut fixture, name);
    fixture.app.acknowledge_persistence(rename, true);
    fixture
}

#[test]
fn rename_preserves_body_editor_and_cancel_preserves_existing_name() {
    let mut fixture = Fixture::new();
    let create = fixture.paste("Body Grüße\n第二行");
    fixture.app.acknowledge_persistence(create, true);
    fixture.input(crate::key_input(UiKey::SelectAll));
    let before = fixture.app.editor_snapshot().expect("body editor").clone();

    assert!(fixture.effects(control_r()).is_empty());
    assert_eq!(fixture.app.editor_snapshot(), Some(before.clone()));
    fixture.input(UiInput::Paste("Temporary".to_owned()));
    fixture.input(crate::key_input(UiKey::Undo));
    fixture.input(crate::key_input(UiKey::Redo));
    fixture.input(crate::key_input(UiKey::Escape));

    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.name, None);
    assert_eq!(thought.content, "Body Grüße\n第二行");
    assert_eq!(fixture.app.editor_snapshot(), Some(before));

    let rename = commit_name(&mut fixture, "Kept name");
    fixture.app.acknowledge_persistence(rename, true);
    fixture.input(control_r());
    fixture.input(crate::key_input(UiKey::SelectAll));
    fixture.input(UiInput::Paste("Discarded".to_owned()));
    fixture.input(crate::key_input(UiKey::Escape));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("Kept name")
    );
}

#[test]
fn commit_sanitizes_unicode_clear_and_board_undo_redo_are_durable() {
    let mut fixture = Fixture::new();
    let create = fixture.paste("exact body");
    fixture.app.acknowledge_persistence(create, true);
    let rename = commit_name(&mut fixture, "  Release\n計画\u{0007}  ");
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("Release 計画")
    );
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact body"
    );
    fixture.app.acknowledge_persistence(rename, true);
    fixture.input(crate::key_input(UiKey::Escape));

    let undo = fixture.effects(crate::key_input(UiKey::Undo));
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove { undo: true, .. }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].name, None);
    let undo_sequence = operation_sequence(&undo);
    fixture.app.acknowledge_persistence(undo_sequence, true);

    let redo = fixture.effects(crate::key_input(UiKey::Redo));
    assert!(matches!(
        redo.as_slice(),
        [Effect::CommitHistoryMove { undo: false, .. }]
    ));
    assert!(fixture.app.state.board.live_thoughts()[0].name.is_some());
    let redo_sequence = operation_sequence(&redo);
    fixture.app.acknowledge_persistence(redo_sequence, true);

    fixture.input(control_r());
    fixture.input(crate::key_input(UiKey::SelectAll));
    fixture.input(crate::key_input(UiKey::Backspace));
    let clear = fixture.effects(crate::key_input(UiKey::Enter));
    assert!(matches!(
        clear.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].name, None);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact body"
    );
}

#[test]
fn name_is_outside_clipboard_body_and_duplicate_preserves_metadata() {
    let mut fixture = named_fixture(UiSettings::default(), "exact body", "Private label");
    fixture.input(crate::key_input(UiKey::Escape));

    let copy = fixture.effects(crate::key_input(UiKey::Copy));
    assert!(matches!(
        copy.as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == "exact body"
    ));

    let duplicate = fixture.effects(crate::key_input(UiKey::Duplicate));
    assert!(matches!(
        duplicate.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    let thoughts = fixture.app.state.board.live_thoughts();
    assert_eq!(thoughts.len(), 2);
    assert_eq!(thoughts[0].content, thoughts[1].content);
    assert_eq!(thoughts[0].name, thoughts[1].name);
}

#[test]
fn title_hit_geometry_opens_the_same_editor_without_entering_body_selection() {
    let mut fixture = named_fixture(UiSettings::default(), "body text", "Clickable title");
    fixture.input(crate::key_input(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 9));
    let thought = layout.thoughts.first().expect("thought layout");
    let name = thought.name.expect("name geometry");
    assert_eq!(
        layout.hit_test(name.x, name.y),
        Some(HitTarget::ThoughtName(thought.thought_id))
    );

    fixture.pointer(name.x, name.y, PointerKind::Move);
    let hovered = draw_theme(&mut fixture, 52, 9, ThemePreference::Dark);
    assert!(
        hovered.backend().buffer()[(name.x, name.y)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );

    fixture.pointer(name.x, name.y, PointerKind::Down(PointerButton::Left));
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Board
    ));
    fixture.input(crate::key_input(UiKey::SelectAll));
    fixture.input(UiInput::Paste("Mouse edited".to_owned()));
    fixture.input(crate::key_input(UiKey::Escape));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("Clickable title")
    );
}

#[test]
fn mouse_places_the_title_cursor_and_exposes_truthful_save_and_cancel_controls() {
    let mut fixture = named_fixture(UiSettings::default(), "body", "AlphaBeta");
    fixture.input(crate::key_input(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 9));
    let title = layout.thoughts[0].name.expect("title geometry");
    fixture.pointer(
        title.x.saturating_add(5),
        title.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Paste("X".to_owned()));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 9));
    let cancel = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::CancelThoughtName).then_some(*area))
        .expect("cancel control");
    assert!(
        layout
            .controls
            .iter()
            .any(|(target, _)| *target == HitTarget::CommitThoughtName)
    );
    fixture.pointer(cancel.x, cancel.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("AlphaBeta")
    );

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 9));
    let title = layout.thoughts[0].name.expect("title geometry");
    fixture.pointer(
        title.x.saturating_add(5),
        title.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Paste("X".to_owned()));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 9));
    let save = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::CommitThoughtName).then_some(*area))
        .expect("save control");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: save.x,
        row: save.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("AlphaXBeta")
    );
}

#[test]
fn clicking_another_title_preserves_the_body_editor_owner_and_outside_click_commits() {
    let mut fixture = Fixture::new();
    let first_create = fixture.paste("first body");
    fixture.app.acknowledge_persistence(first_create, true);
    let first_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(crate::key_input(UiKey::Escape));
    let second_create = fixture.paste("second body");
    fixture.app.acknowledge_persistence(second_create, true);
    let second_id = fixture.app.state.board.live_thoughts()[1].id;
    let second_rename = commit_name(&mut fixture, "Second title");
    fixture.app.acknowledge_persistence(second_rename, true);
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualUp,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::SelectAll));
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { thought_id } if thought_id == first_id
    ));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 10));
    let body_editor = fixture.app.editor_snapshot().expect("first body editor");
    let second_title = layout
        .thought(second_id)
        .and_then(|thought| thought.name)
        .expect("second title geometry");
    fixture.pointer(
        second_title.x,
        second_title.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(crate::key_input(UiKey::SelectAll));
    fixture.input(UiInput::Paste("Renamed by mouse".to_owned()));
    assert_eq!(fixture.app.editor_snapshot(), Some(body_editor.clone()));
    assert_eq!(fixture.app.state.focused_thought, Some(first_id));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 10));
    let first_body = layout
        .thought(first_id)
        .expect("first body geometry")
        .text_area;
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: first_body.x,
        row: first_body.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(second_id)
            .and_then(|thought| thought.name.as_ref())
            .map(ThoughtName::as_str),
        Some("Renamed by mouse")
    );
    assert_eq!(fixture.app.editor_snapshot(), Some(body_editor));
}

#[test]
fn rapid_name_changes_are_ordered_and_stale_acknowledgements_are_ignored() {
    let mut fixture = Fixture::new();
    let create = fixture.paste("durable body");
    fixture.app.acknowledge_persistence(create, true);
    let rename = commit_name(&mut fixture, "First name");

    fixture.input(control_r());
    fixture.input(crate::key_input(UiKey::SelectAll));
    fixture.input(UiInput::Paste("Too soon".to_owned()));
    let successor = fixture.effects(crate::key_input(UiKey::Enter));
    assert!(matches!(
        successor.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if operation.kind == BoardOperationKind::Rename
    ));
    let successor = operation_sequence(&successor);
    assert_eq!(successor.get(), rename.get() + 1);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("Too soon")
    );

    fixture.app.acknowledge_persistence(rename, false);
    assert_eq!(
        fixture.effects(crate::key_input(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence: rename }]
    );
    fixture.app.acknowledge_persistence(rename, true);
    assert!(matches!(
        fixture.app.state.durability,
        DurabilityState::Pending { latest, .. } if latest == successor
    ));
    fixture.app.acknowledge_persistence(successor, true);
    assert!(matches!(
        fixture.app.state.durability,
        DurabilityState::Durable { sequence } if sequence == successor
    ));

    assert!(
        fixture
            .app
            .acknowledge_persistence(rename, false)
            .is_empty()
    );
    assert!(matches!(
        fixture.app.state.durability,
        DurabilityState::Durable { sequence } if sequence == successor
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some("Too soon")
    );
}

#[test]
fn named_thoughts_have_reviewed_comfortable_compact_collapsed_and_clipped_views() {
    assert_comfortable_snapshot();
    assert_compact_collapsed_and_shallow_snapshots();
    assert_narrow_editing_snapshot();
}

fn assert_comfortable_snapshot() {
    let mut comfortable = named_fixture(
        UiSettings::default(),
        "A body that remains visually separate from organizational metadata.",
        "Release 計画",
    );
    comfortable.input(crate::key_input(UiKey::Escape));
    insta::assert_snapshot!(
        "thought_name_comfortable",
        snapshot_buffer(
            draw_theme(&mut comfortable, 58, 9, ThemePreference::Dark)
                .backend()
                .buffer()
        )
    );
}

fn assert_compact_collapsed_and_shallow_snapshots() {
    let compact_settings = UiSettings {
        density: BoardDensity::Compact,
        ..UiSettings::default()
    };
    let mut compact = named_fixture(
        compact_settings,
        "Compact body line one\nline two",
        "Compact title",
    );
    compact.input(crate::key_input(UiKey::Escape));
    insta::assert_snapshot!(
        "thought_name_compact",
        snapshot_buffer(
            draw_theme(&mut compact, 42, 7, ThemePreference::Dark)
                .backend()
                .buffer()
        )
    );

    let thought_id = compact.app.state.board.live_thoughts()[0].id;
    compact
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .presentation = ThoughtPresentation::Collapsed;
    insta::assert_snapshot!(
        "thought_name_collapsed_narrow",
        snapshot_buffer(
            draw_theme(&mut compact, 24, 5, ThemePreference::Dark)
                .backend()
                .buffer()
        )
    );
    insta::assert_snapshot!(
        "thought_name_shallow",
        snapshot_buffer(
            draw_theme(&mut compact, 18, 4, ThemePreference::Dark)
                .backend()
                .buffer()
        )
    );
    compact.input(crate::key_input(UiKey::Enter));
    let mut shallow_editor = draw_theme(&mut compact, 18, 4, ThemePreference::Dark);
    let shallow_layout = compact.app.prepare_frame(Rect::new(0, 0, 18, 4));
    let shallow_body = shallow_layout
        .thought(thought_id)
        .expect("shallow body layout")
        .text_area;
    assert_eq!(shallow_body.height, 1);
    assert_eq!(
        shallow_editor
            .backend_mut()
            .get_cursor_position()
            .expect("visible shallow body cursor")
            .y,
        shallow_body.y
    );
    compact.input(crate::key_input(UiKey::Escape));
}

fn assert_narrow_editing_snapshot() {
    let long = "界".repeat(100);
    let mut editing = named_fixture(UiSettings::default(), "body stays separate", &long);
    assert_eq!(
        editing.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .expect("bounded name")
            .as_str()
            .chars()
            .count(),
        80
    );
    editing.input(control_r());
    let mut editing_terminal = draw_theme(&mut editing, 24, 5, ThemePreference::Dark);
    let title_cursor = editing_terminal
        .backend_mut()
        .get_cursor_position()
        .expect("title cursor");
    assert_eq!(title_cursor.y, 0);
    assert!(title_cursor.x < 24);
    insta::assert_snapshot!(
        "thought_name_editing_narrow",
        snapshot_buffer(editing_terminal.backend().buffer())
    );
}
