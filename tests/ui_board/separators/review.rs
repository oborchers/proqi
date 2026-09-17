use super::*;

#[test]
fn mixed_thought_actions_filter_separators_for_collapse_and_merge() {
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
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        merged
            .fixture
            .app
            .state
            .board
            .thought(merged.first)
            .expect("merged thought")
            .content,
        "first\n\nsecond"
    );
    assert_eq!(
        merged.ids(),
        vec![merged.first.into(), merged.separator.into()]
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
