//! Board Delete alias and text-entry isolation contracts.

use super::navigation::durable_thought;
use super::*;

fn deletion_fixture(contents: &[&str]) -> Fixture {
    let mut fixture = Fixture::new();
    for content in contents {
        durable_thought(&mut fixture, content);
    }
    fixture
}

fn deletion_count(mutation: &proqi::domain::BoardMutation) -> usize {
    match mutation {
        proqi::domain::BoardMutation::Batch { mutations } => {
            mutations.iter().map(deletion_count).sum()
        }
        proqi::domain::BoardMutation::SetDeletion {
            deleted_at: Some(_),
            ..
        } => 1,
        _ => 0,
    }
}

#[test]
fn configured_character_and_physical_delete_share_one_board_operation() {
    for key in [UiKey::Character('d'), UiKey::Delete] {
        let mut fixture = deletion_fixture(&["only thought"]);
        let effects = fixture.effects(UiInput::Key(key));
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("delete spelling must request exactly one board operation: {effects:?}");
        };
        assert_eq!(operation.kind, proqi::domain::BoardOperationKind::Delete);
        assert_eq!(deletion_count(&operation.forward), 1);
        assert!(fixture.app.state.board.live_thoughts().is_empty());

        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Undo));
        assert_eq!(
            fixture.app.state.board.live_thoughts()[0].content,
            "only thought"
        );
    }
}

#[test]
fn configured_character_and_physical_delete_share_bulk_selection_and_undo() {
    for key in [UiKey::Character('d'), UiKey::Delete] {
        let mut fixture = deletion_fixture(&["first", "second", "third"]);
        fixture.input(UiInput::Key(UiKey::Character(' ')));
        fixture.input(UiInput::Key(UiKey::Character('k')));
        fixture.input(UiInput::Key(UiKey::Character(' ')));

        let effects = fixture.effects(UiInput::Key(key));
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("bulk delete spelling must request one operation: {effects:?}");
        };
        assert_eq!(deletion_count(&operation.forward), 2);
        assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "first");

        fixture.input(UiInput::Key(UiKey::Undo));
        let contents = fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>();
        assert_eq!(contents, ["first", "second", "third"]);
    }
}

#[test]
fn physical_delete_is_invariant_while_the_character_binding_remains_remappable() {
    let mut settings = UiSettings::default();
    settings.keybindings.delete = 'z';
    let mut fixture = Fixture::with_settings(settings);
    durable_thought(&mut fixture, "remapped");

    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('d')))
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(matches!(
        fixture.effects(UiInput::Key(UiKey::Delete)).as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));

    fixture.input(UiInput::Key(UiKey::Undo));
    assert!(matches!(
        fixture
            .effects(UiInput::Key(UiKey::Character('z')))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
}

#[test]
fn delete_and_backspace_are_noops_on_explicit_empty_board_and_insertion_row() {
    let mut empty = Fixture::new();
    empty.input(UiInput::Key(UiKey::Escape));
    for key in [UiKey::Delete, UiKey::Backspace, UiKey::Character('d')] {
        assert!(empty.effects(UiInput::Key(key)).is_empty());
        assert!(empty.app.state.board.live_thoughts().is_empty());
    }

    for key in [UiKey::Delete, UiKey::Character('d')] {
        let mut insertion = deletion_fixture(&["existing"]);
        insertion.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }));
        assert!(matches!(
            insertion.effects(UiInput::Key(key)).as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        assert!(insertion.app.state.board.live_thoughts().is_empty());
    }

    let mut insertion = deletion_fixture(&["existing"]);
    insertion.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert!(insertion.effects(UiInput::Key(UiKey::Backspace)).is_empty());
    assert_eq!(insertion.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn delete_remains_forward_text_deletion_and_vim_letters_remain_content() {
    let mut fixture = Fixture::new();
    for character in "hjklab".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert!(fixture.effects(UiInput::Key(UiKey::Delete)).is_empty());
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "hjkla"
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn query_letters_and_delete_never_escape_into_board_commands() {
    let mut fixture = deletion_fixture(&["hjkl target", "other"]);
    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "hjklx".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Delete));

    let (query, _, _) = fixture.app.search_view().expect("search query");
    assert_eq!(query, "hjkl");
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn failed_delete_persistence_retries_the_same_single_operation() {
    let mut fixture = deletion_fixture(&["retry deletion"]);
    fixture.acknowledge_all_persistence();
    let effects = fixture.effects(UiInput::Key(UiKey::Delete));
    let sequence = effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("delete persistence sequence");
    fixture.app.acknowledge_persistence(sequence, false);

    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }]
    );
    assert!(fixture.effects(UiInput::Key(UiKey::Delete)).is_empty());
}

#[test]
fn mixed_delete_spellings_do_not_create_a_second_operation_after_the_board_empties() {
    let mut fixture = deletion_fixture(&["last thought"]);
    fixture.acknowledge_all_persistence();
    let effects = fixture.effects(UiInput::Key(UiKey::Character('d')));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(fixture.effects(UiInput::Key(UiKey::Delete)).is_empty());

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    fixture.input(UiInput::Key(UiKey::Redo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn public_shortcut_documentation_records_the_alias_and_text_entry_boundary() {
    let readme = include_str!("../../README.md");
    assert!(readme.contains("`d` or `Del` (`Entf` on German keyboards)"));
    assert!(readme.contains("Physical `Del` is an invariant Board alias"));
    assert!(readme.contains("`h`, `j`, `k`, and"));
    assert!(readme.contains("`l` remain literal text"));
}
