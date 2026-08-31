use super::*;

#[test]
fn duplicate_mentions_receive_the_smallest_stable_location_qualifier() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![
            reviewer(AgentState::Idle),
            reference(
                "reviewer",
                ("w3", Some("Implementation")),
                ("w3:t2", Some("Review")),
                "w3:p1",
                AgentState::Idle,
            ),
        ],
    );
    app.open_invocation_picker();
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    assert_eq!(
        app.editor_presentation()
            .expect("presentation")
            .snapshot
            .content,
        "@reviewer · codex · Product/p9 "
    );
    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}

#[test]
fn selecting_an_inline_mention_and_pressing_enter_reveals_its_exact_location() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![reviewer(AgentState::Idle)],
    );
    app.open_invocation_picker();
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let thought_id = app.active_thought_id().expect("active thought");
    let annotations = app.current_annotations(thought_id);
    let [annotation] = annotations.as_slice() else {
        panic!("one mention annotation");
    };
    app.set_editor_range(annotation.start, annotation.end);

    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(effects.is_empty());
    assert_eq!(
        app.editor_presentation()
            .expect("expanded presentation")
            .snapshot
            .content,
        "Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}
