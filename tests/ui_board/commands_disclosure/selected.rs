use super::*;

fn populated() -> Fixture {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        saved_thought(&mut fixture, content);
    }
    fixture
}

#[test]
fn selected_move_availability_matches_effective_run_movement() {
    let mut all = populated();
    all.input(key_input(UiKey::Character('a')));
    open(&mut all);
    let (enabled, rendered) = searched_row(&mut all, "Move item up", 72);
    assert!(!enabled);
    assert!(rendered.contains("Selected items are already at the edge"));

    let mut movable = populated();
    movable.input(key_input(UiKey::Character(' ')));
    movable.input(key_input(UiKey::Character('k')));
    movable.input(key_input(UiKey::Character(' ')));
    open(&mut movable);
    assert!(searched_row(&mut movable, "Move item up", 72).0);
}

#[test]
fn selected_text_actions_are_available_for_the_ordered_thought_cohort() {
    for command in [
        "Clean up spacing",
        "Send to another Proqi session",
        "Send to another Proqi session and remove thought",
    ] {
        let mut fixture = populated();
        fixture.input(key_input(UiKey::Character('a')));
        open(&mut fixture);
        assert!(searched_row(&mut fixture, command, 80).0, "{command}");
    }
}
