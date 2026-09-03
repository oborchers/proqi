//! Annotation projection across board and editor clipboard intentions.

use super::{Fixture, draw};
use proqi::{
    application::Effect,
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::CursorMovement,
    ui::{PastePayload, PointerButton, PointerKind, UiInput, UiKey},
};

fn attachment(content: &str, start: usize, end: usize, image: bool) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::Attachment {
            image,
            display_name: content[start..end].to_owned(),
        },
    }
}

#[test]
fn whole_thought_copy_shifts_every_annotation_across_canonical_separators() {
    let first_path = "/offline/Grüße 🖼️.png";
    let first = format!("before {first_path} after");
    let first_start = "before ".len();
    let first_annotation = attachment(&first, first_start, first_start + first_path.len(), true);
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(first.clone(), vec![first_annotation.clone()])
            .expect("first payload"),
    ));
    fixture.input(UiInput::Key(UiKey::Escape));

    let repeated = "/offline/same.txt";
    let second = format!("{repeated} and {repeated}");
    let second_start = repeated.len() + " and ".len();
    let second_annotations = vec![
        attachment(&second, 0, repeated.len(), false),
        attachment(&second, second_start, second_start + repeated.len(), false),
    ];
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(second.clone(), second_annotations.clone())
            .expect("second payload"),
    ));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::SelectAll));

    let effects = fixture.effects(UiInput::Key(UiKey::Copy));
    let [
        Effect::WriteClipboard {
            content,
            annotations,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("clipboard effect");
    };
    assert_eq!(content, &format!("{first}\n\n{second}"));
    let second_offset = first.len() + 2;
    assert_eq!(
        annotations,
        &[
            first_annotation,
            ContentAnnotation {
                start: second_offset,
                end: second_offset + repeated.len(),
                kind: second_annotations[0].kind.clone(),
            },
            ContentAnnotation {
                start: second_offset + second_start,
                end: second_offset + second_start + repeated.len(),
                kind: second_annotations[1].kind.clone(),
            },
        ]
    );
}

#[test]
fn collapsed_placeholder_copy_and_cut_use_the_complete_canonical_range() {
    let path = "/missing/界 image.png";
    let content = format!("prefix {path} suffix");
    let start = "prefix ".len();
    let annotation = attachment(&content, start, start + path.len(), true);
    let mut fixture = Fixture::with_annotated_thought(&content, vec![annotation.clone()]);
    fixture.input(UiInput::Key(UiKey::Enter));
    let _terminal = draw(&mut fixture, 60, 8);
    let area = fixture
        .app
        .prepare_frame(ratatui_core::layout::Rect::new(0, 0, 60, 8))
        .thoughts[0]
        .text_area;
    fixture.pointer(
        area.x + u16::try_from("prefix ".len()).expect("column"),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );

    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        copy.as_slice(),
        [Effect::WriteClipboard { content, annotations, .. }]
            if content == path
                && annotations == &[ContentAnnotation {
                    start: 0,
                    end: path.len(),
                    kind: annotation.kind.clone(),
                }]
    ));

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("cut write");
    };
    let deletion =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert!(matches!(deletion.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "prefix  suffix"
    );
    assert!(
        fixture.app.state.board.live_thoughts()[0]
            .annotations
            .is_empty()
    );
}

#[test]
fn shrinking_a_keyboard_selection_inside_an_attachment_keeps_cut_atomic() {
    let path = "/missing/界 image.png";
    let content = format!("prefix {path} suffix");
    let start = "prefix ".len();
    let annotation = attachment(&content, start, start + path.len(), true);
    let mut fixture = Fixture::with_annotated_thought(&content, vec![annotation]);
    fixture.input(UiInput::Key(UiKey::Enter));
    let _terminal = draw(&mut fixture, 60, 8);
    let area = fixture
        .app
        .prepare_frame(ratatui_core::layout::Rect::new(0, 0, 60, 8))
        .thoughts[0]
        .text_area;
    fixture.pointer(
        area.x + u16::try_from(start).expect("column"),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: true,
    }));

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [
        Effect::WriteClipboard {
            request_id,
            content: copied,
            annotations,
            ..
        },
    ] = cut.as_slice()
    else {
        panic!("cut write");
    };
    assert_eq!(copied, path);
    assert_eq!(annotations.len(), 1);
    let deletion =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert!(matches!(deletion.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "prefix  suffix"
    );
}

#[test]
fn extending_a_keyboard_selection_across_a_large_paste_keeps_its_metadata() {
    let payload = "line one\nline two\nline three";
    let content = format!("prefix {payload} suffix");
    let start = "prefix ".len();
    let annotation = ContentAnnotation {
        start,
        end: start + payload.len(),
        kind: ContentAnnotationKind::LargePaste {
            lines: 3,
            graphemes: payload.chars().count(),
        },
    };
    let mut fixture = Fixture::with_annotated_thought(&content, vec![annotation.clone()]);
    fixture.input(UiInput::Key(UiKey::Enter));
    let _terminal = draw(&mut fixture, 60, 8);
    let area = fixture
        .app
        .prepare_frame(ratatui_core::layout::Rect::new(0, 0, 60, 8))
        .thoughts[0]
        .text_area;
    fixture.pointer(
        area.x + u16::try_from(start).expect("column"),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: true,
    }));

    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        copy.as_slice(),
        [Effect::WriteClipboard { content, annotations, .. }]
            if content == &format!("{payload} ")
                && annotations == &[ContentAnnotation {
                    start: 0,
                    end: payload.len(),
                    kind: annotation.kind,
                }]
    ));
}
