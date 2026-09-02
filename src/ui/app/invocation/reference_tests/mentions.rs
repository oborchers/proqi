use super::*;

#[test]
fn duplicate_mentions_receive_the_smallest_stable_location_qualifier() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(
        &mut app,
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
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 80, 8));

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
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let thought_id = app.active_thought_id().expect("active thought");
    let annotations = app.current_annotations(thought_id);
    let [annotation] = annotations.as_slice() else {
        panic!("one mention annotation");
    };
    app.set_editor_range(annotation.start, annotation.end);

    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(effects.is_empty());
    app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 80, 8));
    assert_eq!(
        app.editor_presentation()
            .expect("expanded presentation")
            .snapshot
            .content,
        "Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}

#[test]
fn maximum_length_duplicate_mention_remains_valid_and_persistable() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    let workspace_a = format!("{}a", "w".repeat(59));
    let workspace_b = format!("{}b", "w".repeat(59));
    let pane_a = format!("{workspace_a}:p1");
    let pane_b = format!("{workspace_b}:p1");
    let tab_a = format!("{workspace_a}:t1");
    let tab_b = format!("{workspace_b}:t1");
    let name = "n".repeat(32);
    let harness = "h".repeat(32);
    open_with_live(
        &mut app,
        vec![
            reference_kind(
                Some(&name),
                &harness,
                (&workspace_a, Some("Same workspace")),
                (&tab_a, Some("Same tab")),
                &pane_a,
                AgentState::Idle,
            ),
            reference_kind(
                Some(&name),
                &harness,
                (&workspace_b, Some("Same workspace")),
                (&tab_b, Some("Same tab")),
                &pane_b,
                AgentState::Idle,
            ),
        ],
    );
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    let thought_id = app.active_thought_id().expect("active thought");
    let annotations = app.current_annotations(thought_id);
    let [annotation] = annotations.as_slice() else {
        panic!("one mention annotation");
    };
    let ContentAnnotationKind::InvocationReference { display_name } = &annotation.kind else {
        panic!("invocation reference annotation");
    };
    assert!(display_name.ends_with(&format!("pane {pane_a}")));
    assert!(display_name.chars().count() <= 256);

    let effects = app.flush_pending_edit(&mut ids, &clock);
    assert_eq!(effects.len(), 1);
    assert!(!app.has_pending_edit());
}
