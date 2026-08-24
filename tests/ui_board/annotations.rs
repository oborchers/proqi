use super::*;

fn image_payload(path: &str) -> PastePayload {
    PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "screenshot.png".to_owned(),
            },
        }],
    )
}

#[test]
fn image_path_folds_on_the_board_but_every_exact_content_path_is_preserved() {
    let mut fixture = Fixture::new();
    let path = "/private/temporary/location/screenshot.png";
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, path);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].annotations.len(),
        1
    );

    fixture.input(UiInput::Key(UiKey::Escape));
    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Image 1]  screenshot.png"));
    assert!(!rendered.contains("/private/temporary"));

    let effects = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        effects.as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == path
    ));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, path);
}

#[test]
fn large_paste_is_folded_until_editing_and_editor_undo_restores_its_fold() {
    let mut fixture = Fixture::new();
    let content = (0..14)
        .map(|line| format!("context line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content.clone()));
    fixture.input(UiInput::Key(UiKey::Escape));
    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Pasted text]  14 lines"));
    assert!(!rendered.contains("context line 13"));

    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let effects = fixture.effects(UiInput::Key(UiKey::Undo));
    assert_eq!(effects.len(), 2);
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Pasted text]  14 lines"));
}
