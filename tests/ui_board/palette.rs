use super::*;

#[test]
fn command_palette_is_searchable_and_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.paste("existing");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "quit".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let terminal = draw(&mut fixture, 40, 12);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains(":quit"));
    assert!(rendered.contains("Quit Proqi"));

    let quit = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 40, 12))
        .overlay
        .expect("command overlay")
        .items[0];
    fixture.pointer(quit.x, quit.y, PointerKind::Down(PointerButton::Left));
    assert!(fixture.app.quit);
}

#[test]
fn palette_quit_is_global_and_shallow_navigation_stays_visible() {
    let mut fixture = Fixture::new();
    fixture.paste("existing");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    let _terminal = draw(&mut fixture, 30, 5);
    for _ in 0..10 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }));
    }
    let _terminal = draw(&mut fixture, 30, 5);
    let (_, visible, selected) = fixture.app.palette_view().expect("palette");
    assert!(selected < visible.len());

    fixture.input(UiInput::Key(UiKey::Quit));
    assert!(fixture.app.quit);
}

#[test]
fn palette_query_accepts_normalized_paste_and_grapheme_cursor_edits() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    fixture.input(UiInput::Paste("qu\nit".to_owned()));
    let (query, _, _) = fixture.app.palette_view().expect("palette");
    assert_eq!(query, "qu it");

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let (query, _, _) = fixture.app.palette_view().expect("palette");
    assert_eq!(query, "qu i!t");
}

#[test]
fn palette_exposes_an_explicit_update_check() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "check for updates".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let (_, entries, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(entries, vec!["Check for updates"]);
    assert_eq!(selected, 0);
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(
        effects,
        vec![Effect::Update(proqi::application::UpdateIntent::CheckNow)]
    );
}
