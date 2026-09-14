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
}
