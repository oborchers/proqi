//! Fold-aware wrapped visual-row selection through keyboard, resize, and pointer input.

use super::{Fixture, draw, navigation};
use proqi::{
    application::Effect,
    domain::{ContentAnnotation, ContentAnnotationKind, TextPosition},
    ports::editor::{CursorMovement, TextSelection, VisualLine},
    ui::{HitTarget, PointerButton, PointerKind, UiKey, UiSettings, VisualRowEdge},
};
use ratatui_core::layout::Rect;
use unicode_segmentation::UnicodeSegmentation as _;

#[cfg(target_os = "macos")]
use proqi::ports::editor::VisualCursorAffinity;

fn extend(fixture: &mut Fixture, edge: VisualRowEdge) {
    fixture.input(crate::key_input(UiKey::ExtendVisualRow { edge }));
}

#[cfg(target_os = "macos")]
fn move_to_edge(fixture: &mut Fixture, edge: VisualRowEdge) {
    fixture.input(crate::key_input(UiKey::MoveVisualRow { edge }));
}

fn move_to_grapheme(fixture: &mut Fixture, grapheme: usize) {
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..grapheme {
        fixture.input(crate::key_input(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
}

fn row_position(row: &VisualLine, end: bool) -> TextPosition {
    TextPosition::new(
        row.logical_line,
        if end {
            row.end_grapheme
        } else {
            row.start_grapheme
        },
    )
}

#[test]
fn repeated_chords_extend_both_selection_directions_across_wrapped_unicode_rows() {
    let content = "tab\t界 e\u{301} 👩🏽‍💻 control\u{7} abcdefghijklmnopqrstuvwxyz";
    let mut fixture = Fixture::new();
    fixture.paste(content);
    let _frame = draw(&mut fixture, 18, 9);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    assert!(rows.len() >= 4, "expected wrapped Unicode rows: {rows:?}");

    let backward_anchor = rows[2].start_grapheme + 1;
    move_to_grapheme(&mut fixture, backward_anchor);
    extend(&mut fixture, VisualRowEdge::Start);
    assert_eq!(
        fixture.app.editor_snapshot().expect("first start").cursor,
        row_position(&rows[2], false)
    );
    extend(&mut fixture, VisualRowEdge::Start);
    let backward = fixture.app.editor_snapshot().expect("second start");
    assert_eq!(backward.cursor, row_position(&rows[1], false));
    assert_eq!(
        backward.selection,
        Some(TextSelection {
            start: row_position(&rows[1], false),
            end: TextPosition::new(0, backward_anchor),
        })
    );

    let forward_anchor = rows[0].start_grapheme + 1;
    move_to_grapheme(&mut fixture, forward_anchor);
    extend(&mut fixture, VisualRowEdge::End);
    assert_eq!(
        fixture.app.editor_snapshot().expect("first end").cursor,
        row_position(&rows[0], true)
    );
    extend(&mut fixture, VisualRowEdge::End);
    let forward = fixture.app.editor_snapshot().expect("second end");
    assert_eq!(forward.cursor, row_position(&rows[1], true));
    assert_eq!(
        forward.selection,
        Some(TextSelection {
            start: TextPosition::new(0, forward_anchor),
            end: row_position(&rows[1], true),
        })
    );
}

#[test]
#[cfg(target_os = "macos")]
fn unshifted_row_edge_movement_uses_the_current_wrapped_row_without_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("0123456789 abcdefghijklmnopqrstuvwxyz");
    let _frame = draw(&mut fixture, 16, 8);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    assert!(rows.len() >= 3, "expected wrapping: {rows:?}");
    for index in [0, 1, rows.len() - 1] {
        let row = &rows[index];
        move_to_grapheme(&mut fixture, row.start_grapheme + 1);

        move_to_edge(&mut fixture, VisualRowEdge::Start);
        let at_start = fixture.app.editor_snapshot().expect("row start");
        assert_eq!(at_start.cursor, row_position(row, false));
        assert_eq!(at_start.selection, None);
        move_to_edge(&mut fixture, VisualRowEdge::Start);
        assert_eq!(
            fixture
                .app
                .editor_snapshot()
                .expect("stable row start")
                .cursor,
            row_position(row, false)
        );

        move_to_edge(&mut fixture, VisualRowEdge::End);
        let at_end = fixture.app.editor_snapshot().expect("row end");
        assert_eq!(at_end.cursor, row_position(row, true));
        let expected_affinity = if row.start_byte < row.end_byte
            && rows
                .get(index + 1)
                .is_some_and(|next| next.start_byte == row.end_byte)
        {
            VisualCursorAffinity::PreviousRow
        } else {
            VisualCursorAffinity::NextRow
        };
        assert_eq!(at_end.cursor_affinity, expected_affinity);
        assert_eq!(at_end.selection, None);
        move_to_edge(&mut fixture, VisualRowEdge::End);
        assert_eq!(
            fixture
                .app
                .editor_snapshot()
                .expect("stable row end")
                .cursor,
            row_position(row, true)
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
fn unshifted_visual_row_end_renders_at_the_current_row_boundary() {
    use ratatui_core::backend::Backend as _;

    let mut fixture = Fixture::new();
    fixture.paste("alpha beta gamma delta epsilon zeta eta theta");
    let _frame = draw(&mut fixture, 16, 8);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    let (row_index, row) = rows
        .iter()
        .enumerate()
        .find(|(index, row)| *index > 0 && *index + 1 < rows.len() && row.cell_width < 14)
        .expect("interior row ending before the wrap width");
    move_to_grapheme(&mut fixture, row.start_grapheme + 1);

    move_to_edge(&mut fixture, VisualRowEdge::End);

    let canonical = fixture.app.editor_snapshot().expect("row end");
    assert_eq!(canonical.cursor, row_position(row, true));
    assert_eq!(canonical.cursor_affinity, VisualCursorAffinity::PreviousRow);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 16, 8)).thoughts[0].text_area;
    let mut terminal = draw(&mut fixture, 16, 8);
    let rendered = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("rendered cursor");
    assert_eq!(
        (rendered.x, rendered.y),
        (
            area.x + u16::try_from(row.cell_width).expect("row width"),
            area.y + u16::try_from(row_index).expect("row index"),
        )
    );
}

#[test]
#[cfg(target_os = "macos")]
fn exact_width_visual_row_end_uses_the_last_cell_without_advancing() {
    use ratatui_core::backend::Backend as _;

    let mut fixture = Fixture::new();
    fixture.paste(&"x".repeat(56));
    let _frame = draw(&mut fixture, 16, 8);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    let row = &rows[1];
    assert_eq!(row.cell_width, 14);
    move_to_grapheme(&mut fixture, row.start_grapheme + 1);

    move_to_edge(&mut fixture, VisualRowEdge::End);
    move_to_edge(&mut fixture, VisualRowEdge::End);

    let canonical = fixture.app.editor_snapshot().expect("exact-width end");
    assert_eq!(canonical.cursor, row_position(row, true));
    assert_eq!(canonical.cursor_affinity, VisualCursorAffinity::PreviousRow);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 16, 8)).thoughts[0].text_area;
    let mut terminal = draw(&mut fixture, 16, 8);
    let rendered = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("rendered exact-width cursor");
    assert_eq!((rendered.x, rendered.y), (area.right() - 1, area.y + 1));

    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    let moved = fixture.app.editor_snapshot().expect("next exact-width row");
    assert_eq!(moved.cursor, row_position(&rows[2], true));
    assert_eq!(moved.cursor_affinity, VisualCursorAffinity::PreviousRow);
}

#[test]
fn shifted_visual_row_edges_reverse_around_one_stable_anchor() {
    let mut fixture = Fixture::new();
    fixture.paste("alpha beta gamma delta epsilon zeta eta theta");
    let _frame = draw(&mut fixture, 16, 8);
    let rows = fixture.app.editor_snapshot().expect("editor").visual_lines;
    let anchor = rows[1].start_grapheme + 2;
    move_to_grapheme(&mut fixture, anchor);

    extend(&mut fixture, VisualRowEdge::End);
    assert_eq!(
        fixture.app.editor_snapshot().expect("forward").selection,
        Some(TextSelection {
            start: TextPosition::new(0, anchor),
            end: row_position(&rows[1], true),
        })
    );
    extend(&mut fixture, VisualRowEdge::Start);
    assert_eq!(
        fixture.app.editor_snapshot().expect("reversed").selection,
        Some(TextSelection {
            start: row_position(&rows[1], false),
            end: TextPosition::new(0, anchor),
        })
    );
    extend(&mut fixture, VisualRowEdge::Start);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("past anchor")
            .selection,
        Some(TextSelection {
            start: row_position(&rows[0], false),
            end: TextPosition::new(0, anchor),
        })
    );
}

#[test]
fn resize_reflow_retargets_the_current_rendered_row_without_moving_the_anchor() {
    let mut fixture = Fixture::new();
    fixture.paste("0123456789 界界 e\u{301} emoji 👩‍💻 abcdefghijklmnopqrstuvwxyz");
    let _wide = draw(&mut fixture, 34, 8);
    move_to_grapheme(&mut fixture, 18);
    let anchor = fixture.app.editor_snapshot().expect("wide cursor").cursor;

    let _narrow = draw(&mut fixture, 16, 8);
    let before = fixture.app.editor_snapshot().expect("narrow editor");
    assert_eq!(before.cursor, anchor);
    let row = before
        .visual_lines
        .iter()
        .find(|row| row.start_grapheme <= anchor.grapheme && anchor.grapheme < row.end_grapheme)
        .expect("reflowed cursor row")
        .clone();
    extend(&mut fixture, VisualRowEdge::Start);
    let after = fixture.app.editor_snapshot().expect("selection");
    assert_eq!(after.cursor, row_position(&row, false));
    assert_eq!(after.selection.expect("range").end, anchor);
}

#[test]
fn logical_delimiters_empty_rows_and_trailing_newline_have_directional_boundaries() {
    let mut fixture = Fixture::new();
    fixture.paste("ab\r\n\r\n界\n");
    let _frame = draw(&mut fixture, 80, 8);
    move_to_grapheme(&mut fixture, 1);
    for expected in [
        TextPosition::new(0, 2),
        TextPosition::new(1, 0),
        TextPosition::new(2, 1),
        TextPosition::new(3, 0),
    ] {
        extend(&mut fixture, VisualRowEdge::End);
        assert_eq!(
            fixture.app.editor_snapshot().expect("forward").cursor,
            expected
        );
    }
    for expected in [
        TextPosition::new(2, 0),
        TextPosition::new(1, 0),
        TextPosition::new(0, 0),
    ] {
        extend(&mut fixture, VisualRowEdge::Start);
        assert_eq!(
            fixture.app.editor_snapshot().expect("reverse").cursor,
            expected
        );
    }
}

fn substitution(kind: ContentAnnotationKind) -> ContentAnnotation {
    ContentAnnotation {
        start: 0,
        end: "canonical folded payload".len(),
        kind,
    }
}

#[test]
fn collapsed_substitutions_are_atomic_and_expanded_folds_use_exact_content_rows() {
    let content = "canonical folded payload";
    for annotation in [
        substitution(ContentAnnotationKind::Attachment {
            ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
            image: false,
            display_name: "payload.txt".to_owned(),
        }),
        substitution(ContentAnnotationKind::LargePaste {
            lines: 12,
            graphemes: 1_234,
        }),
        substitution(ContentAnnotationKind::InvocationReference {
            display_name: "@reviewer · codex".to_owned(),
        }),
    ] {
        let mut fixture = Fixture::with_annotated_thought(content, vec![annotation]);
        fixture.input(crate::key_input(UiKey::Enter));
        let _frame = draw(&mut fixture, 13, 8);
        move_to_grapheme(&mut fixture, 0);
        extend(&mut fixture, VisualRowEdge::End);
        let snapshot = fixture.app.editor_snapshot().expect("collapsed");
        assert_eq!(
            snapshot.cursor,
            TextPosition::new(0, content.graphemes(true).count())
        );
        assert_eq!(
            snapshot.selection,
            Some(TextSelection {
                start: TextPosition::default(),
                end: TextPosition::new(0, content.graphemes(true).count()),
            })
        );
    }

    let mut expanded = Fixture::with_annotated_thought(
        content,
        vec![substitution(ContentAnnotationKind::LargePaste {
            lines: 12,
            graphemes: content.graphemes(true).count(),
        })],
    );
    expanded.input(crate::key_input(UiKey::Enter));
    let _collapsed = draw(&mut expanded, 13, 8);
    move_to_grapheme(&mut expanded, 0);
    expanded.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    expanded.input(crate::key_input(UiKey::Enter));
    let _expanded = draw(&mut expanded, 13, 8);
    move_to_grapheme(&mut expanded, 2);
    extend(&mut expanded, VisualRowEdge::End);
    let endpoint = expanded.app.editor_snapshot().expect("expanded").cursor;
    assert!(endpoint.grapheme > 2);
    assert!(endpoint.grapheme < content.graphemes(true).count());
}

#[test]
fn configured_fallback_and_mouse_anchor_share_the_same_undo_neutral_selection_path() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_legacy(&proqi::ui::KeyBindings {
            select_visual_row_end: 'R',
            ..proqi::ui::KeyBindings::default()
        })
        .expect("valid translated keymap"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    let sequence = fixture.paste("mouse anchored wrapped content abcdefghijklmnopqrstuvwxyz");
    let _ack = fixture.app.acknowledge_persistence(sequence, true);
    let _frame = draw(&mut fixture, 20, 8);
    fixture.input(crate::key_input(UiKey::Character('!')));
    let effects = fixture.effects(crate::key_input(UiKey::PrimaryShiftCharacter('R')));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::CommitRevision(_)))
            .count(),
        1
    );
    let _typed = draw(&mut fixture, 20, 8);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 20, 8)).thoughts[0].text_area;
    fixture.pointer(
        area.x.saturating_add(3),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(
        area.x.saturating_add(3),
        area.y,
        PointerKind::Up(PointerButton::Left),
    );
    let anchor = fixture.app.editor_snapshot().expect("mouse cursor").cursor;
    let effects = fixture.effects(crate::key_input(UiKey::PrimaryShiftCharacter('R')));
    assert!(effects.is_empty());
    let selected = fixture.app.editor_snapshot().expect("fallback selection");
    assert_eq!(selected.selection.expect("selection").start, anchor);
    assert!(
        fixture
            .app
            .flush_pending_edit(&mut fixture.ids, &fixture.clock)
            .is_empty()
    );
}

#[test]
fn visual_row_selection_palette_fallback_is_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.paste("0123456789 abcdefghijklmnopqrstuvwxyz");
    let _editor = draw(&mut fixture, 20, 8);
    move_to_grapheme(&mut fixture, 3);
    let expected = fixture.app.editor_snapshot().expect("editor").visual_lines[0].end_grapheme;
    fixture.input(crate::key_input(UiKey::Escape));
    let commands = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 20, 8))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    fixture.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    for character in "visual row end".chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    let _palette = draw(&mut fixture, 20, 8);
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 20, 8))
        .overlay
        .expect("command overlay")
        .items[0];
    fixture.pointer(item.x, item.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        fixture.app.editor_snapshot().expect("selection").selection,
        Some(TextSelection {
            start: TextPosition::new(0, 3),
            end: TextPosition::new(0, expected),
        })
    );
}

#[test]
fn visual_row_selection_intentions_do_not_change_board_navigation_or_selection() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        navigation::durable_thought(&mut fixture, content);
    }
    let focused = fixture.app.state.focused_thought;
    for key in [
        UiKey::ExtendVisualRow {
            edge: VisualRowEdge::Start,
        },
        UiKey::ExtendVisualRow {
            edge: VisualRowEdge::End,
        },
        UiKey::MoveVisualRow {
            edge: VisualRowEdge::Start,
        },
        UiKey::MoveVisualRow {
            edge: VisualRowEdge::End,
        },
    ] {
        assert!(fixture.effects(crate::key_input(key)).is_empty());
        assert_eq!(fixture.app.state.focused_thought, focused);
    }
}
