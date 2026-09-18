//! Optional thought-name interaction, payload separation, and responsive rendering.

#[path = "optional_thought_names/views.rs"]
mod views;

use super::*;

use proqi::{
    application::{DurabilityState, InteractionMode},
    domain::{BoardOperationKind, ThoughtName},
};
use ratatui_core::style::Modifier;

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

    let resting = draw_theme(&mut fixture, 52, 9, ThemePreference::Dark);
    let resting_style = resting.backend().buffer()[(name.x, name.y)].style();
    assert_eq!(
        resting_style.fg,
        Some(proqi::ui::Theme::resolve(ThemePreference::Dark, true).muted)
    );
    assert!(resting_style.add_modifier.contains(Modifier::BOLD));
    assert!(!resting_style.add_modifier.contains(Modifier::UNDERLINED));

    fixture.pointer(name.x, name.y, PointerKind::Move);
    let hovered = draw_theme(&mut fixture, 52, 9, ThemePreference::Dark);
    assert_eq!(
        hovered.backend().buffer()[(name.x, name.y)].style(),
        resting_style,
        "title hover must preserve its restrained baseline typography"
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
fn hidden_footer_keeps_thought_name_actions_visible_and_hit_testable() {
    let settings = UiSettings {
        footer_hidden: true,
        ..UiSettings::default()
    };
    let mut fixture = named_fixture(settings, "body", "AlphaBeta");
    fixture.input(crate::key_input(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 4));
    let title = layout.thoughts[0].name.expect("title geometry");
    fixture.pointer(title.x, title.y, PointerKind::Down(PointerButton::Left));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 4));
    assert_eq!(layout.footer_actions.height, 1);
    let mut cancel = None;
    for target in [HitTarget::CommitThoughtName, HitTarget::CancelThoughtName] {
        let area = layout
            .controls
            .iter()
            .find_map(|(candidate, area)| (*candidate == target).then_some(*area))
            .expect("visible thought-name action");
        assert_eq!(layout.hit_test(area.x, area.y), Some(target));
        if target == HitTarget::CancelThoughtName {
            cancel = Some(area);
        }
    }
    let cancel = cancel.expect("cancel geometry");
    fixture.pointer(cancel.x, cancel.y, PointerKind::Down(PointerButton::Left));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 52, 4));
    assert!(layout.controls.iter().all(|(target, _)| {
        !matches!(
            target,
            HitTarget::CommitThoughtName | HitTarget::CancelThoughtName
        )
    }));
}

#[test]
fn clipped_title_click_uses_the_visible_prefix_and_narrow_views_keep_both_controls() {
    let original = "abcdefghijklmnopqrstuvwxyz".repeat(3);
    let mut fixture = named_fixture(UiSettings::default(), "body", &original);
    fixture.input(crate::key_input(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 24, 5));
    let title = layout.thoughts[0].name.expect("clipped title geometry");
    fixture.pointer(
        title.x.saturating_add(2),
        title.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Paste("X".to_owned()));
    let effects = fixture.effects(crate::key_input(UiKey::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    let expected = format!("{}X{}", &original[..2], &original[2..]);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0]
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        Some(expected.as_str())
    );

    for width in 8..=16 {
        fixture.input(control_r());
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, 5));
        for target in [HitTarget::CommitThoughtName, HitTarget::CancelThoughtName] {
            let area = layout
                .controls
                .iter()
                .find_map(|(candidate, area)| (*candidate == target).then_some(*area))
                .expect("both title controls remain visible");
            assert_eq!(layout.hit_test(area.x, area.y), Some(target));
        }
        fixture.input(crate::key_input(UiKey::Escape));
    }
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
    assert_eq!(fixture.app.state.focused_thought_id(), Some(first_id));

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
    insta::assert_snapshot!("thought_name_comfortable", views::comfortable());
    let (compact, collapsed) = views::compact_and_collapsed();
    insta::assert_snapshot!("thought_name_compact", compact);
    insta::assert_snapshot!("thought_name_collapsed_narrow", collapsed);
    insta::assert_snapshot!("thought_name_shallow", views::shallow());
    insta::assert_snapshot!("thought_name_editing_narrow", views::editing_narrow());
}
