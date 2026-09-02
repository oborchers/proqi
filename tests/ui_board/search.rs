//! Thought-search query, result, and focus-transition contracts.

use super::{Fixture, UiInput, UiKey, draw, text};
use proqi::ui::FastNavigation;

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

#[test]
fn thought_search_fast_navigation_moves_five_filtered_entries_and_clamps() {
    let mut fixture = Fixture::new();
    for index in 0..9 {
        fixture.paste(&format!("search row {index} 界 e\u{301}"));
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "search".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let (_, all, _) = fixture.app.search_view().expect("search");
    let expected = all[5].clone();
    let _ = draw(&mut fixture, 30, 6);
    fixture.input(UiInput::Key(UiKey::FastNavigation {
        direction: FastNavigation::Next,
        extend_selection: false,
    }));
    let (_, visible, selected) = fixture.app.search_view().expect("search");
    assert_eq!(visible[selected], expected);

    fixture.input(UiInput::Key(UiKey::FastNavigation {
        direction: FastNavigation::Previous,
        extend_selection: false,
    }));
    let (_, visible, selected) = fixture.app.search_view().expect("search");
    assert_eq!(visible[selected], all[0]);
}
