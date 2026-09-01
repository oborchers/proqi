//! Large-content and mixed-collapse-state placeholder Space regressions.

use unicode_segmentation::UnicodeSegmentation as _;

use super::{
    ContentAnnotationKind, CursorMovement, Fixture, UiInput, UiKey, annotated, assert_shifted,
    attachment, draw, move_key, select_forward, space_revision, substitution, text,
};

#[test]
fn space_preserves_an_expanded_sibling_while_shifting_the_collapsed_target() {
    let expanded = "/tmp/expanded.png";
    let collapsed = "/tmp/collapsed.txt";
    let content = format!("{expanded}|{collapsed}");
    let second_start = expanded.len() + 1;
    let mut fixture = Fixture::with_annotated_thought(
        &content,
        vec![
            substitution(attachment(true), 0, expanded.len()),
            substitution(attachment(false), second_start, content.len()),
        ],
    );
    select_forward(&mut fixture, "");
    fixture.input(UiInput::Key(UiKey::Enter));
    assert!(text(draw(&mut fixture, 42, 7).backend().buffer()).contains(expanded));

    fixture.input(move_key(CursorMovement::DocumentEnd, false));
    fixture.input(move_key(CursorMovement::GraphemeBack, false));
    let revision = space_revision(&mut fixture);
    assert_eq!(revision.after_content, format!("{expanded}| {collapsed}"));
    assert_eq!(revision.after_annotations[0].start, 0);
    assert_eq!(revision.after_annotations[1].start, second_start + 1);
    let rendered = text(draw(&mut fixture, 42, 7).backend().buffer());
    assert!(rendered.contains(expanded), "{rendered:?}");
    assert!(!rendered.contains(collapsed), "{rendered:?}");
    assert!(rendered.contains("[File "), "{rendered:?}");
}

#[test]
fn threshold_sized_large_paste_keeps_exact_bytes_range_and_cursor_after_resize() {
    let line = "界e\u{301}\t[]{}".repeat(30);
    let value = (0..12)
        .map(|index| format!("{index:02}:{line}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    let graphemes = value.graphemes(true).count();
    assert!(graphemes >= 1_200);
    let prefix = "α ";
    let suffix = "\r\nΩ";
    let mut fixture = annotated(
        prefix,
        &value,
        suffix,
        ContentAnnotationKind::LargePaste {
            lines: 12,
            graphemes,
        },
    );
    select_forward(&mut fixture, prefix);
    fixture.input(UiInput::Resize {
        width: 18,
        height: 5,
    });
    let before_resize = text(draw(&mut fixture, 18, 5).backend().buffer());
    assert!(before_resize.contains("characters]"), "{before_resize:?}");

    let before = format!("{prefix}{value}{suffix}");
    let expected = format!("{prefix} {value}{suffix}");
    let revision = space_revision(&mut fixture);
    assert_shifted(&fixture, &revision, &before, &expected, prefix.len());
    assert_eq!(
        revision.after_annotations[0].end,
        prefix.len() + 1 + value.len()
    );
    let after_resize = text(draw(&mut fixture, 18, 5).backend().buffer());
    assert!(after_resize.contains("[Pasted text"), "{after_resize:?}");
}
