use super::*;

fn selected_contents(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| fixture.app.thought_selected(thought.id))
        .map(|thought| thought.content.as_str())
        .collect()
}

fn populate(fixture: &mut Fixture) {
    for content in ["first", "Grüße 👩‍💻", "第二行"] {
        fixture.paste(content);
        fixture.input(crate::key_input(UiKey::Escape));
    }
}

#[test]
fn configurable_board_select_all_is_ordered_idempotent_and_escape_clears_it() {
    let mut fixture = Fixture::new();
    populate(&mut fixture);
    fixture.input(crate::key_input(UiKey::Character(' ')));

    fixture.input(crate::key_input(UiKey::Character('a')));
    assert_eq!(selected_contents(&fixture), ["first", "Grüße 👩‍💻", "第二行"]);
    fixture.input(crate::key_input(UiKey::Character('a')));
    assert_eq!(selected_contents(&fixture), ["first", "Grüße 👩‍💻", "第二行"]);

    fixture.input(crate::key_input(UiKey::Escape));
    assert!(selected_contents(&fixture).is_empty());

    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_legacy(&proqi::ui::KeyBindings {
            select_all: 'z',
            ..proqi::ui::KeyBindings::default()
        })
        .expect("valid translated keymap"),
        ..UiSettings::default()
    };
    let mut remapped = Fixture::with_settings(settings);
    populate(&mut remapped);
    remapped.input(crate::key_input(UiKey::Character('a')));
    assert!(selected_contents(&remapped).is_empty());
    remapped.input(crate::key_input(UiKey::Character('z')));
    assert_eq!(
        selected_contents(&remapped),
        ["first", "Grüße 👩‍💻", "第二行"]
    );
}

#[test]
fn forwarded_primary_a_selects_the_board_but_keeps_edit_mode_text_selection() {
    let mut fixture = Fixture::new();
    populate(&mut fixture);

    fixture.input(crate::key_input(UiKey::SelectAll));
    assert_eq!(selected_contents(&fixture), ["first", "Grüße 👩‍💻", "第二行"]);

    fixture.input(crate::key_input(UiKey::Enter));
    assert!(selected_contents(&fixture).is_empty());
    fixture.input(crate::key_input(UiKey::SelectAll));
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    let selection = snapshot.selection.expect("complete text selection");
    assert_eq!(selection.start, proqi::domain::TextPosition::default());
    assert_eq!(selection.end, proqi::domain::TextPosition::new(0, 3));
}

#[test]
fn select_all_works_from_the_insertion_row_and_is_empty_on_an_empty_board() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::SelectAll));
    assert!(selected_contents(&fixture).is_empty());

    populate(&mut fixture);
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert!(fixture.app.insertion_focused());
    fixture.input(crate::key_input(UiKey::SelectAll));
    assert_eq!(selected_contents(&fixture), ["first", "Grüße 👩‍💻", "第二行"]);
    fixture.input(crate::key_input(UiKey::Escape));
    assert!(selected_contents(&fixture).is_empty());
}

#[test]
fn command_palette_exposes_the_exact_select_all_items_action() {
    let mut fixture = Fixture::new();
    populate(&mut fixture);
    fixture.input(crate::key_input(UiKey::Character(':')));
    for character in "select all items".chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    let (_, entries, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(entries, vec!["Select all items"]);
    assert_eq!(selected, 0);

    fixture.input(crate::key_input(UiKey::Enter));

    assert_eq!(selected_contents(&fixture), ["first", "Grüße 👩‍💻", "第二行"]);
}
