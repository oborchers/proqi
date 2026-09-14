//! Walkthrough feedback for contextual reflow and transform disclosure.

use super::*;

#[test]
fn focused_thought_promotes_reflow_with_f_and_demotes_session_utilities() {
    let mut fixture = Fixture::new();
    saved_thought(&mut fixture, "ordinary saved thought");
    open(&mut fixture);

    let (_, rows, _) = fixture.app.palette_view().expect("concise Commands");
    assert!(rows.contains(&"Clean up spacing".to_owned()));
    assert!(!rows.contains(&"Rename session".to_owned()));
    assert!(!rows.contains(&"Copy resume command".to_owned()));

    let rendered = text(draw(&mut fixture, 80, 16).backend().buffer());
    assert!(rendered.contains("Clean up spacing"));
    assert!(rendered.contains("f · board"));
}

#[test]
fn contextual_transform_binding_labels_only_the_applicable_exact_intention() {
    let mut merge = Fixture::new();
    for thought in ["one", "two"] {
        saved_thought(&mut merge, thought);
    }
    merge.input(crate::key_input(UiKey::UnmodifiedSpace));
    merge.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualUp,
        extend_selection: false,
    }));
    merge.input(crate::key_input(UiKey::UnmodifiedSpace));
    open(&mut merge);

    let (_, rows, _) = merge.app.palette_view().expect("merge Commands");
    assert!(rows.contains(&"Merge selected thoughts".to_owned()));
    assert!(!rows.iter().any(|row| row == "Transform"));
    let rendered = text(draw(&mut merge, 80, 16).backend().buffer());
    assert!(rendered.contains("Merge selected thoughts"));
    assert!(rendered.contains("t · selection"));

    let mut split = Fixture::new();
    saved_thought(&mut split, "split here");
    split.input(crate::key_input(UiKey::Character('e')));
    split.input(crate::key_input(UiKey::Escape));
    open(&mut split);

    let (_, rows, _) = split.app.palette_view().expect("split Commands");
    assert!(rows.contains(&"Split thought at cursor".to_owned()));
    assert!(!rows.contains(&"Extract selection as new thought".to_owned()));
    let rendered = text(draw(&mut split, 80, 16).backend().buffer());
    assert!(rendered.contains("Split thought at cursor"));
    assert!(rendered.contains("t · edit"));

    let mut selected_split = Fixture::new();
    saved_thought(&mut selected_split, "selected split source");
    selected_split.input(crate::key_input(UiKey::Character('e')));
    selected_split.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    selected_split.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    selected_split.input(crate::key_input(UiKey::Escape));
    open(&mut selected_split);
    let (_, split_rendered) = searched_row(&mut selected_split, "Split thought at cursor", 88);
    let split_line = split_rendered
        .lines()
        .rfind(|line| line.contains("Split thought at cursor"))
        .expect("Split row");
    assert!(!split_line.contains("t ·"));

    let mut selected_extract = Fixture::new();
    saved_thought(&mut selected_extract, "selected extract source");
    selected_extract.input(crate::key_input(UiKey::Character('e')));
    selected_extract.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    selected_extract.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    assert!(
        selected_extract
            .app
            .editor_snapshot()
            .and_then(|snapshot| snapshot.selection)
            .is_some()
    );
    selected_extract.input(crate::key_input(UiKey::Escape));
    open(&mut selected_extract);
    let (extract_enabled, extract_rendered) = searched_row(
        &mut selected_extract,
        "Extract selection as new thought",
        88,
    );
    assert!(extract_enabled);
    let extract_line = extract_rendered
        .lines()
        .rfind(|line| line.contains("Extract selection as new thought"))
        .expect("Extract row");
    assert!(extract_line.contains("t · edit"), "{extract_line}");
}

#[test]
fn reflow_and_query_history_show_their_effective_input_owners() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"commands.open\"=[{key='F9'}]\n[bindings.commands]\n\"history.undo\"=[{key='F12'}]",
        )
        .expect("context-owned Commands bindings"),
        ..UiSettings::default()
    };
    let mut reflow = Fixture::with_settings(settings.clone());
    saved_thought(&mut reflow, "editor reflow source");
    reflow.input(crate::key_input(UiKey::Character('e')));
    reflow.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut reflow, "Clean up spacing", 88);
    assert!(rendered.contains("Ctrl+Shift+F · edit"));
    assert!(!rendered.contains("f · board"));

    let mut history = Fixture::with_settings(settings);
    saved_thought(&mut history, "query history source");
    open(&mut history);
    let (_, rendered) = searched_row(&mut history, "Undo", 88);
    assert!(rendered.contains("F12 · commands"));
}

#[test]
fn equal_ranked_search_results_use_the_stable_typed_action_tiebreaker() {
    let mut fixture = Fixture::new();
    saved_thought(&mut fixture, "ranking source");
    open(&mut fixture);
    type_query(&mut fixture, "submit");
    assert_eq!(
        fixture.app.palette_view().expect("ranked Commands").1,
        [
            "Submit",
            "Submit to agent...",
            "Submit all",
            "Submit all and keep",
            "Submit and keep",
        ]
    );
}
