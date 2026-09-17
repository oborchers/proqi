//! Passive hover feedback for modal controls and authoritative current geometry.

use super::*;

fn area_for(layout: &proqi::ui::LayoutSnapshot, target: HitTarget) -> Rect {
    layout
        .controls
        .iter()
        .find_map(|(candidate, area)| (*candidate == target).then_some(*area))
        .expect("visible control")
}

#[test]
fn help_close_control_receives_passive_motion_without_closing() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "hover target");
    let board = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    let help = board
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::Help).then_some(*area))
        .expect("Help footer control");
    fixture.pointer(help.x, help.y, PointerKind::Down(PointerButton::Left));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    let close = layout.overlay.expect("Help overlay").close;

    fixture.pointer(close.x, close.y, PointerKind::Move);

    assert_eq!(fixture.app.hovered(), Some(HitTarget::CloseOverlay));
    assert!(
        fixture.app.help,
        "hover must not activate the close control"
    );
}

#[test]
fn footer_hover_tracks_edges_crossings_repeats_and_focus_without_actions() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "hover target");
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    let commands = area_for(&layout, HitTarget::Commands);
    let help = area_for(&layout, HitTarget::Help);
    let original_mode = fixture.app.interaction_mode();
    let original_content = fixture.app.state.board.live_thoughts()[0].content.clone();

    for column in [commands.x, commands.right().saturating_sub(1), commands.x] {
        let effects = fixture.effects(UiInput::Pointer(PointerInput {
            column,
            row: commands.y,
            kind: PointerKind::Move,
            extend_selection: false,
        }));
        assert!(effects.is_empty());
        assert_eq!(fixture.app.hovered(), Some(HitTarget::Commands));
    }
    fixture.pointer(help.x, help.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), Some(HitTarget::Help));
    fixture.pointer(layout.area.right(), help.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None);
    assert_eq!(fixture.app.interaction_mode(), original_mode);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        original_content
    );

    fixture.pointer(
        layout.thoughts[0].gutter.x,
        layout.thoughts[0].gutter.y,
        PointerKind::Move,
    );
    let terminal = draw_theme(&mut fixture, 42, 12, ThemePreference::Limited);
    let gutter =
        &terminal.backend().buffer()[(layout.thoughts[0].gutter.x, layout.thoughts[0].gutter.y)];
    assert!(
        gutter
            .modifier
            .contains(ratatui_core::style::Modifier::UNDERLINED)
    );
}

#[test]
fn current_frame_reconciles_hover_after_resize_and_modal_overlap() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "hover target");
    let wide = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    let help = area_for(&wide, HitTarget::Help);
    fixture.pointer(help.x, help.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), Some(HitTarget::Help));

    let narrow = fixture.app.prepare_frame(Rect::new(0, 0, 18, 5));
    assert_eq!(fixture.app.hovered(), narrow.hit_test(help.x, help.y));

    let commands = narrow
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::Commands).then_some(*area));
    if let Some(commands) = commands {
        fixture.pointer(
            commands.x,
            commands.y,
            PointerKind::Down(PointerButton::Left),
        );
    } else {
        fixture.input(crate::key_input(UiKey::Shortcut(
            proqi::ui::ShortcutActionId::OpenCommands,
        )));
    }
    let overlay = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 18, 5))
        .overlay
        .expect("Commands overlay");
    fixture.pointer(overlay.area.x, overlay.area.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None, "overlay border is passive");
    fixture.pointer(overlay.close.x, overlay.close.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), Some(HitTarget::CloseOverlay));

    fixture.input(UiInput::HostFocusLost);
    assert_eq!(fixture.app.hovered(), None);
}

#[test]
fn collapsed_fold_hover_uses_projected_identity_and_preserves_exact_content() {
    let mut fixture = Fixture::new();
    let content = (0..14)
        .map(|line| format!("folded row {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content.clone()));
    fixture.input(crate::key_input(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8));
    let thought = &layout.thoughts[0];

    fixture.pointer(thought.text_area.x, thought.text_area.y, PointerKind::Move);

    let thought_id = thought.thought_id;
    assert_eq!(fixture.app.hovered(), Some(HitTarget::Fold(thought_id, 0)));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let cell = &terminal.backend().buffer()[(thought.text_area.x, thought.text_area.y)];
    assert!(
        cell.modifier
            .contains(ratatui_core::style::Modifier::UNDERLINED)
    );

    fixture.pointer(
        thought.text_area.x,
        thought.text_area.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("fold editor")
            .selection
            .is_some()
    );
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}

#[test]
fn scrolled_edit_fold_hover_uses_the_same_visible_projection_as_activation() {
    let mut fixture = Fixture::new();
    let prefix = (0..18)
        .map(|line| format!("visible context {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let path = "/tmp/scrolled-screenshot.png";
    let content = format!("{prefix}\n{path}");
    let fold_start = prefix.len() + 1;
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(
            content.clone(),
            vec![ContentAnnotation {
                start: fold_start,
                end: fold_start + path.len(),
                kind: ContentAnnotationKind::Attachment {
                    ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
                    image: true,
                    display_name: "scrolled-screenshot.png".to_owned(),
                },
            }],
        )
        .expect("valid annotated payload"),
    ));
    let area = Rect::new(0, 0, 48, 8);
    let layout = fixture.app.prepare_frame(area);
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("scrolled editor")
            .scroll_row
            > 0
    );
    let thought = layout.thoughts[0].clone();
    let fold_row = (thought.text_area.y..thought.text_area.bottom())
        .find(|row| {
            fixture.pointer(thought.text_area.x, *row, PointerKind::Move);
            matches!(fixture.app.hovered(), Some(HitTarget::Fold(_, 0)))
        })
        .expect("visible collapsed fold");
    let thought_id = thought.thought_id;
    assert_eq!(fixture.app.hovered(), Some(HitTarget::Fold(thought_id, 0)));

    fixture.pointer(
        thought.text_area.x,
        fold_row,
        PointerKind::Down(PointerButton::Left),
    );

    let selected = fixture.app.editor_snapshot().expect("fold selection");
    assert!(selected.selection.is_some());
    assert_eq!(selected.content, content);
}

#[test]
fn passive_overlay_regions_and_disabled_rows_never_gain_hover() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Character(':')));
    for character in "delete thought".chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 18));
    let overlay = layout.overlay.expect("Commands overlay");
    let disabled = overlay
        .item_interactive
        .iter()
        .zip(&overlay.items)
        .find_map(|(interactive, area)| (!interactive).then_some(*area))
        .expect("at least one unavailable command");
    fixture.pointer(disabled.x, disabled.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None);

    for _ in 0.."delete thought".chars().count() {
        fixture.input(crate::key_input(UiKey::Backspace));
    }
    let (_, rows, _) = fixture.app.palette_view().expect("Commands rows");
    let more = rows
        .iter()
        .position(|row| row == "More commands...")
        .expect("disclosure control");
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 18));
    let more = layout.overlay.expect("concise Commands").items[more];
    fixture.pointer(more.x, more.y, PointerKind::Down(PointerButton::Left));
    let expanded = fixture.app.prepare_frame(Rect::new(0, 0, 72, 20));
    let heading = expanded
        .overlay
        .expect("expanded Commands")
        .item_headings
        .iter()
        .flatten()
        .next()
        .copied()
        .expect("category heading");
    fixture.pointer(heading.x, heading.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None);
}

#[test]
fn scrolling_and_async_footer_refresh_reconcile_at_the_pointer_cell() {
    let mut fixture = Fixture::new();
    for index in 0..10 {
        super::navigation::durable_thought(&mut fixture, &format!("thought {index}"));
    }
    let area = Rect::new(0, 0, 42, 8);
    let first = fixture.app.prepare_frame(area);
    let point = first.thoughts[0].text_area;
    fixture.pointer(point.x, point.y, PointerKind::Move);
    fixture.pointer(point.x, point.y, PointerKind::ScrollDown);
    let scrolled = fixture.app.prepare_frame(area);
    assert_eq!(fixture.app.hovered(), scrolled.hit_test(point.x, point.y));

    let commands = area_for(&scrolled, HitTarget::Commands);
    fixture.pointer(commands.x, commands.y, PointerKind::Move);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(
            proqi::domain::Direction::Left,
            "w1:p2",
        )]));
    let refreshed = fixture.app.prepare_frame(area);
    assert_eq!(
        fixture.app.hovered(),
        refreshed.hit_test(commands.x, commands.y)
    );
}

#[test]
fn restricted_pointer_owners_hover_only_controls_they_can_activate() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    fixture.input(crate::key_input(UiKey::Character(' ')));
    fixture.app.complete_agent_discovery(Ok(vec![
        super::agent::target(proqi::domain::Direction::Up, "w1:p2"),
        super::agent::target(proqi::domain::Direction::Right, "w1:p3"),
    ]));
    fixture.input(crate::submission_input::control_submit(true));
    assert_eq!(
        fixture.app.submission_mode(),
        Some(proqi::ports::agent::SubmissionDisposition::Keep)
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 12));
    let thought = layout.thoughts[0].text_area;
    fixture.pointer(thought.x, thought.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None);
    assert!(fixture.app.thought_selected(layout.thoughts[0].thought_id));

    let rename = area_for(&layout, HitTarget::RenameSession);
    fixture.pointer(rename.x, rename.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), None);

    let (delivery, area) = layout
        .controls
        .iter()
        .find(|(target, _)| matches!(target, HitTarget::Deliver(_, _)))
        .copied()
        .expect("submission direction control");
    fixture.pointer(area.x, area.y, PointerKind::Move);
    assert_eq!(fixture.app.hovered(), Some(delivery));

    let mut recovery_fixture = Fixture::new();
    let sequence = recovery_fixture.paste("recovery owner");
    recovery_fixture
        .app
        .acknowledge_persistence(sequence, false);
    let recovery = recovery_fixture.app.prepare_frame(Rect::new(0, 0, 100, 12));
    let thought = recovery.thoughts[0].text_area;
    recovery_fixture.pointer(thought.x, thought.y, PointerKind::Move);
    assert_eq!(recovery_fixture.app.hovered(), None);
    let retry = area_for(&recovery, HitTarget::Retry);
    recovery_fixture.pointer(retry.x, retry.y, PointerKind::Move);
    assert_eq!(recovery_fixture.app.hovered(), Some(HitTarget::Retry));
}

#[test]
fn motion_preserves_armed_keyboard_boundary_state() {
    let mut insertion = Fixture::new();
    super::navigation::durable_thought(&mut insertion, "existing");
    insertion.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(insertion.app.insertion_focused());
    insertion.input(super::navigation::visual(CursorMovement::VisualDown, false));
    let layout = insertion.app.prepare_frame(Rect::new(0, 0, 50, 12));
    let help = area_for(&layout, HitTarget::Help);
    insertion.pointer(help.x, help.y, PointerKind::Move);
    let effects = insertion.effects(super::navigation::visual(CursorMovement::VisualDown, false));
    assert_eq!(effects.len(), 1);
    assert_eq!(insertion.app.state.board.live_thoughts().len(), 2);

    let mut editor = Fixture::new();
    super::navigation::durable_thought(&mut editor, "first");
    super::navigation::durable_thought(&mut editor, "second");
    let first = editor.app.state.board.live_thoughts()[0].id;
    editor.input(crate::key_input(UiKey::Enter));
    editor.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    editor.input(super::navigation::visual(CursorMovement::VisualUp, false));
    let layout = editor.app.prepare_frame(Rect::new(0, 0, 50, 12));
    let point = layout.thoughts[0].text_area;
    editor.pointer(point.x, point.y, PointerKind::Move);
    editor.input(super::navigation::visual(CursorMovement::VisualUp, false));
    assert_eq!(editor.app.state.focused_thought, Some(first));
    assert_eq!(
        editor.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
}

#[test]
fn hovered_board_footer_has_a_reviewable_dark_narrow_buffer() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "hover snapshot");
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    let commands = area_for(&layout, HitTarget::Commands);
    fixture.pointer(commands.x, commands.y, PointerKind::Move);

    insta::assert_snapshot!(super::snapshot_support::snapshot_buffer(
        draw_theme(&mut fixture, 42, 12, ThemePreference::Dark)
            .backend()
            .buffer()
    ));
}

#[test]
fn hovered_recovery_control_has_a_reviewable_limited_shallow_buffer() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("recovery hover");
    fixture.app.acknowledge_persistence(sequence, false);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 6));
    let retry = area_for(&layout, HitTarget::Retry);
    fixture.pointer(retry.x, retry.y, PointerKind::Move);

    insta::assert_snapshot!(super::snapshot_support::snapshot_buffer(
        draw_theme(&mut fixture, 50, 6, ThemePreference::Limited)
            .backend()
            .buffer()
    ));
}
