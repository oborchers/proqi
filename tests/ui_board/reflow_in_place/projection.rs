//! Retained text positions must not inherit whole-paragraph replacement edges.

use super::{Fixture, key_input, reflow};
use proqi::{domain::TextPosition, ports::editor::CursorMovement, ui::UiKey};
use unicode_segmentation::UnicodeSegmentation as _;

fn move_cursor(fixture: &mut Fixture, movement: CursorMovement, extend_selection: bool) {
    fixture.input(key_input(UiKey::Move {
        movement,
        extend_selection,
    }));
}

fn start_at(fixture: &mut Fixture, graphemes: usize) {
    move_cursor(fixture, CursorMovement::DocumentStart, false);
    for _ in 0..graphemes {
        move_cursor(fixture, CursorMovement::GraphemeForward, false);
    }
}

#[test]
fn an_interior_caret_stays_with_retained_text_after_whitespace_cleanup() {
    for newline in ["\n", "\r\n"] {
        let mut fixture = Fixture::new();
        fixture.paste(&format!("alpha{newline}beta"));
        start_at(&mut fixture, 2);
        reflow(&mut fixture);
        let snapshot = fixture.app.editor_snapshot().expect("editor");
        assert_eq!(
            snapshot.cursor,
            TextPosition {
                line: 0,
                grapheme: 2
            }
        );
        assert!(snapshot.selection.is_none());
        fixture.input(key_input(UiKey::Character('X')));
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").content,
            format!("alXpha{newline}beta")
        );
    }
}

#[test]
fn partial_selections_keep_exact_unicode_words_in_both_directions_and_expanded_folds() {
    for (newline, expanded, backwards) in [
        ("\n", false, false),
        ("\r\n", false, true),
        ("\n", true, true),
        ("\r\n", true, false),
    ] {
        let tail = if expanded {
            " tail".repeat(300)
        } else {
            String::new()
        };
        let source = format!("alpha{newline}界e\u{301}👩🏽‍💻 beta{tail}");
        let expected = format!("alpha{newline}界e\u{301}👩🏽‍💻 beta{tail}");
        let mut fixture = Fixture::new();
        fixture.paste(&source);
        if expanded {
            move_cursor(&mut fixture, CursorMovement::GraphemeBack, false);
            fixture.input(key_input(UiKey::Enter));
        }
        let word = source.find("beta").expect("word");
        let count = source[..word].graphemes(true).count();
        start_at(&mut fixture, count + if backwards { 4 } else { 0 });
        for _ in 0..4 {
            move_cursor(
                &mut fixture,
                if backwards {
                    CursorMovement::GraphemeBack
                } else {
                    CursorMovement::GraphemeForward
                },
                true,
            );
        }
        reflow(&mut fixture);
        let snapshot = fixture.app.editor_snapshot().expect("editor");
        assert_eq!(snapshot.content, expected);
        let selected = snapshot.selection.expect("partial selection");
        assert_eq!((selected.start.line, selected.end.line), (1, 1));
        let selected_line = snapshot
            .content
            .split('\n')
            .nth(selected.start.line)
            .expect("selected logical line")
            .trim_end_matches('\r');
        assert_eq!(
            selected_line
                .graphemes(true)
                .skip(selected.start.grapheme)
                .take(selected.end.grapheme - selected.start.grapheme)
                .collect::<String>(),
            "beta"
        );
        assert_eq!(
            snapshot.cursor,
            if backwards {
                selected.start
            } else {
                selected.end
            }
        );
        fixture.input(key_input(UiKey::Character('Ω')));
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").content,
            expected.replace("beta", "Ω")
        );
    }
}

#[test]
fn retained_backslashes_and_word_boundaries_keep_exact_carets_and_selections() {
    for (source, after, start, length) in [
        ("alpha  \\\\", "alpha \\\\", 8, 0),
        ("alpha  \\\\", "alpha \\\\", 7, 1),
        ("alpha  \nbeta", "alpha\nbeta", 0, 5),
        ("alpha  \r\nbeta", "alpha\r\nbeta", 5, 0),
    ] {
        for backwards in [false, true] {
            let mut fixture = Fixture::new();
            fixture.paste(source);
            start_at(&mut fixture, start + if backwards { length } else { 0 });
            let movement = if backwards {
                CursorMovement::GraphemeBack
            } else {
                CursorMovement::GraphemeForward
            };
            for _ in 0..length {
                move_cursor(&mut fixture, movement, true);
            }
            reflow(&mut fixture);
            let snapshot = fixture.app.editor_snapshot().expect("editor");
            assert_eq!(snapshot.content, after);
            let mapped = if source.starts_with("alpha  ") && start > 5 {
                start - 1
            } else {
                start
            };
            assert_eq!(
                snapshot.cursor.grapheme,
                mapped + if backwards { 0 } else { length }
            );
            if length > 0 {
                let range = snapshot.selection.expect("selection");
                assert_eq!(
                    (range.start.grapheme, range.end.grapheme),
                    (mapped, mapped + length)
                );
            } else {
                assert!(snapshot.selection.is_none());
            }
        }
    }
}
