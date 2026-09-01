//! Thought-search query, result, and focus-transition contracts.

use super::{Fixture, UiInput, UiKey, draw, text};

#[test]
fn thought_search_filters_content_and_focuses_the_selected_match() {
    let mut fixture = Fixture::new();
    fixture.paste("first searchable prompt");
    let first = fixture.app.state.focused_thought.expect("first thought");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("unrelated second prompt");
    fixture.input(UiInput::Key(UiKey::Escape));

    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "searchable".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let terminal = draw(&mut fixture, 40, 10);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("/searchable"));
    assert!(rendered.contains("first searchable prompt"));
    let (_, results, _) = fixture.app.search_view().expect("search view");
    assert_eq!(results, ["first searchable prompt"]);

    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.state.focused_thought, Some(first));
    assert!(fixture.app.search_view().is_none());
}
