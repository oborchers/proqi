use unicode_segmentation::UnicodeSegmentation as _;

use crate::domain::TextPosition;

use super::{
    LogicalLine, VisibleCellWindow, byte_for_position, cursor_cell, ellipsize_cells, logical_lines,
    position_for_byte, terminal_cell_width, truncate_cells, visible_cell_window, wrap_rows,
};

#[test]
fn terminal_cell_helpers_share_tabs_controls_and_unicode_width() {
    let value = "a\te\u{301}界👩‍💻\u{7}";
    assert_eq!(terminal_cell_width(value), 10);
    assert_eq!(cursor_cell(value, "a\te\u{301}".len()), 5);
    assert_eq!(cursor_cell(value, "a\te".len()), 4);
    assert_eq!(truncate_cells(value, 7), "a   e\u{301}界");
    assert_eq!(truncate_cells(value, 10), "a   e\u{301}界👩‍💻�");
    assert_eq!(ellipsize_cells("\t\u{7}", 5), "    �");
}

#[test]
fn visible_window_keeps_cursor_in_cells_and_never_splits_graphemes() {
    let value = "ab\t界e\u{301}👩‍💻\u{7}z";
    let at_emoji = "ab\t界e\u{301}👩‍💻".len();
    assert_eq!(
        visible_cell_window(value, at_emoji, 6),
        VisibleCellWindow {
            text: "界e\u{301}👩‍💻�".to_owned(),
            cursor_cell: 5,
            start_cell: 4,
        }
    );
    assert_eq!(
        visible_cell_window(value, "ab\t界e".len(), 4),
        VisibleCellWindow {
            text: "  界".to_owned(),
            cursor_cell: 4,
            start_cell: 2,
        }
    );
    assert_eq!(visible_cell_window(value, at_emoji, 0).cursor_cell, 0);
}

#[test]
fn ordinary_words_wrap_at_the_latest_whitespace_boundary() {
    let rows = wrap_rows("Explain the smallest next step", 12);
    let visible = rows
        .iter()
        .map(|row| row.visual.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible, ["Explain the ", "smallest ", "next step"]);
    assert_eq!(rows[1].start_byte, "Explain the ".len());
}

#[test]
fn oversized_unicode_tokens_still_hard_wrap_without_splitting_graphemes() {
    let rows = wrap_rows("界界界e\u{301}界", 4);
    assert_eq!(rows[0].visual.text, "界界");
    assert_eq!(rows[1].visual.text, "界e\u{301}");
    assert_eq!(rows[2].visual.text, "界");
    assert_eq!(rows[1].visual.start_grapheme, 2);
}

#[test]
fn nonbreaking_spaces_do_not_become_wrap_boundaries() {
    let rows = wrap_rows("alpha\u{a0}beta gamma", 9);
    let visible = rows
        .iter()
        .map(|row| row.visual.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible, ["alpha\u{a0}bet", "a gamma"]);

    let narrow = wrap_rows("one\u{202f}two three", 7);
    assert_eq!(narrow[0].visual.text, "one\u{202f}two");
}

#[test]
fn mandatory_unicode_separators_create_distinct_logical_rows() {
    let content = "one\u{2028}two\u{2029}three";
    let rows = wrap_rows(content, 80);
    let visible = rows
        .iter()
        .map(|row| row.visual.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible, ["one", "two", "three"]);
    assert_eq!(
        position_for_byte(content, "one\u{2028}".len()),
        TextPosition::new(1, 0)
    );
}

#[test]
fn trailing_newlines_round_trip_as_distinct_logical_insertion_points() {
    let content = "line\n\n";
    for position in [
        TextPosition::new(0, 4),
        TextPosition::new(1, 0),
        TextPosition::new(2, 0),
    ] {
        let byte = byte_for_position(content, position);
        assert_eq!(position_for_byte(content, byte), position);
    }
}

#[test]
fn every_logical_grapheme_position_round_trips_through_its_canonical_byte() {
    for content in ["", "line\n", "line\n\n", "e\u{301}\n界🙂\n", "one\r\ntwo"] {
        for (line_index, line) in logical_lines(content).into_iter().enumerate() {
            assert_line_positions_round_trip(content, line_index, line);
        }
    }
}

fn assert_line_positions_round_trip(content: &str, line_index: usize, line: LogicalLine) {
    let graphemes = content[line.start..line.content_end]
        .graphemes(true)
        .count();
    for grapheme in 0..=graphemes {
        let position = TextPosition::new(line_index, grapheme);
        let byte = byte_for_position(content, position);
        assert_eq!(position_for_byte(content, byte), position, "{content:?}");
    }
}
