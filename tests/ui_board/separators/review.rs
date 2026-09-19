use super::*;

#[test]
fn mixed_thought_actions_filter_separators_for_collapse_but_not_merge_contiguity() {
    let mut collapsed = Mixed::new();
    collapsed.select_all();
    collapsed.fixture.app.prepare_frame(Rect::new(0, 0, 80, 20));
    let effects = collapsed
        .fixture
        .effects(crate::key_input(UiKey::Character('c')));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!([collapsed.first, collapsed.second].into_iter().all(|id| {
        collapsed
            .fixture
            .app
            .state
            .board
            .thought(id)
            .is_some_and(|thought| {
                thought.presentation == proqi::domain::ThoughtPresentation::Collapsed
            })
    }));
    assert!(
        collapsed
            .fixture
            .app
            .state
            .board
            .separator(collapsed.separator)
            .is_some_and(Separator::is_live)
    );

    let mut merged = Mixed::new();
    merged.select_all();
    let effects = merged
        .fixture
        .effects(crate::key_input(UiKey::Character('t')));
    assert!(effects.is_empty());
    assert_eq!(
        merged
            .fixture
            .app
            .state
            .board
            .thought(merged.first)
            .expect("first thought")
            .content,
        "first"
    );
    assert_eq!(
        merged.ids(),
        vec![
            merged.first.into(),
            merged.separator.into(),
            merged.second.into()
        ]
    );
    assert!(
        merged
            .fixture
            .app
            .status_text()
            .is_some_and(|message| message.contains("contiguous"))
    );
}

#[test]
fn commands_use_eligible_thoughts_when_a_selected_separator_is_focused() {
    let mut cut = Mixed::new();
    cut.select_all();
    assert!(matches!(
        open_command(&mut cut.fixture, "cut thought text").as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == "first\n\nsecond"
    ));

    let mut submit = Mixed::new();
    submit.select_all();
    submit
        .fixture
        .app
        .complete_agent_discovery(Ok(vec![target()]));
    assert!(matches!(
        open_command(&mut submit.fixture, "submit").as_slice(),
        [Effect::PrepareSubmission(attempt)]
            if attempt.sources.iter().map(|source| source.thought_id).collect::<Vec<_>>()
                == vec![submit.first, submit.second]
    ));
}

#[test]
fn rename_requires_the_focused_item_to_be_a_thought_even_with_thoughts_selected() {
    let mut mixed = Mixed::new();
    mixed.select_all();
    mixed.fixture.input(crate::key_input(UiKey::Character(':')));
    for character in "rename thought".chars() {
        mixed
            .fixture
            .input(crate::key_input(UiKey::Character(character)));
    }
    let (_, rows, _) = mixed.fixture.app.palette_view().expect("Commands");
    let rename = rows
        .iter()
        .position(|row| row == "Rename thought")
        .expect("discoverable rename");
    let layout = mixed.fixture.app.prepare_frame(Rect::new(0, 0, 72, 16));
    assert!(!layout.overlay.expect("Commands geometry").item_interactive[rename]);
    mixed.fixture.input(crate::key_input(UiKey::Escape));

    let effects = mixed.fixture.effects(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Character('r')).with_modifiers(LogicalModifiers::CONTROL),
    ));
    assert!(effects.is_empty());
    assert_eq!(
        mixed.fixture.app.state.focused_item,
        Some(mixed.separator.into())
    );
    assert_eq!(
        mixed.fixture.app.status_text(),
        Some("No thought is focused")
    );
    assert!([mixed.first, mixed.second].into_iter().all(|id| {
        mixed
            .fixture
            .app
            .state
            .board
            .thought(id)
            .is_some_and(|thought| thought.name.is_none())
    }));
}
