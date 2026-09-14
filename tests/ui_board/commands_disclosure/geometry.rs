use super::*;

#[test]
fn below_actionable_height_renders_one_explicit_state_and_enter_cannot_execute() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    open(&mut fixture);
    let before = fixture.app.state.clone();

    let rendered = text(draw(&mut fixture, 30, 3).backend().buffer());
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 30, 3));
    let overlay = layout.overlay.expect("Commands geometry");
    assert!(overlay.items.is_empty());
    assert!(overlay.item_interactive.is_empty());
    assert!(rendered.contains("Pane too small"));
    insta::assert_snapshot!("commands_explicit_too_small", trim_snapshot_rows(&rendered));

    assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
    assert_eq!(fixture.app.state, before);
    assert!(fixture.app.palette_view().is_some());
    fixture.input(crate::key_input(UiKey::Character('n')));
    assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
    assert_eq!(fixture.app.state, before);
    fixture.input(crate::key_input(UiKey::Escape));
    assert!(fixture.app.palette_view().is_none());
}
