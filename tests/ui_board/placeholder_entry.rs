use super::*;

#[test]
fn reentered_paste_placeholder_expands_and_untouched_exit_refolds_it() {
    let mut fixture = Fixture::new();
    let content = (0..14)
        .map(|line| format!("context line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content.clone()));
    let _folded = draw(&mut fixture, 60, 8);
    fixture.input(UiInput::Key(UiKey::Escape));
    let _board = draw(&mut fixture, 60, 8);
    fixture.input(UiInput::Key(UiKey::Enter));
    let _reentered = draw(&mut fixture, 60, 8);

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));
    let expanded = draw(&mut fixture, 60, 8);
    let rendered = text(expanded.backend().buffer());
    let thought = &fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0];

    assert!(thought.text_area.height > 1);
    assert!(rendered.contains("context line 13"));
    assert!(!rendered.contains("[Pasted text"));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        content
    );

    fixture.input(UiInput::Key(UiKey::Escape));
    let collapsed = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(collapsed.contains("[Pasted text · 14 lines · 213 characters]"));
    assert!(!collapsed.contains("context line 13"));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}
