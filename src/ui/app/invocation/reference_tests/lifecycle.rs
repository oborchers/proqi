use super::*;

#[test]
fn command_surface_refreshes_live_references_without_rescanning_filesystem_entries() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    app.open_palette();
    app.handle(
        UiInput::Paste("Insert discovered invocation".to_owned()),
        &mut ids,
        &clock,
    );
    let (_, matches, _) = app.palette_view().expect("palette");
    assert_eq!(matches, ["Insert discovered invocation"]);

    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(app.invocation_view().is_some());
    assert!(matches!(
        effects.as_slice(),
        [Effect::DiscoverInvocationReferences(_)]
    ));
}

#[test]
fn newest_result_wins_over_stale_success_and_failure() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    let old_effects = app.open_invocation_picker();
    let [Effect::DiscoverInvocationReferences(old)] = old_effects.as_slice() else {
        panic!("old live refresh");
    };
    let old_generation = old.generation;
    let newest_effects = app.open_invocation_picker();
    let [Effect::DiscoverInvocationReferences(newest)] = newest_effects.as_slice() else {
        panic!("new live refresh");
    };
    let newest_generation = newest.generation;
    app.complete_invocation_reference_discovery(InvocationReferenceDiscovery {
        generation: newest_generation,
        references: Ok(Vec::new()),
    });
    app.complete_invocation_reference_discovery(InvocationReferenceDiscovery {
        generation: old_generation,
        references: Ok(vec![reviewer(AgentState::Working)]),
    });
    assert!(app.invocation_view().expect("picker").1.is_empty());

    let stale = app.open_invocation_picker();
    let newest = app.open_invocation_picker();
    complete_live(&mut app, &newest, Ok(vec![reviewer(AgentState::Idle)]));
    complete_live(&mut app, &stale, Err(AgentFailureCode::TimedOut));
    assert_eq!(app.invocation_view().expect("picker").1.len(), 1);
}

#[test]
fn picker_snapshot_stays_stable_until_reopened() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(&mut app, vec![reviewer(AgentState::Working)]);
    assert!(
        app.invocation_view().expect("picker").1[0]
            .qualifier
            .ends_with("working")
    );

    let filesystem = app.refresh_invocations();
    let [Effect::DiscoverInvocations(request)] = filesystem.as_slice() else {
        panic!("filesystem refresh");
    };
    app.complete_invocation_discovery(Ok(InvocationDiscovery {
        generation: request.generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: Vec::new(),
    }));
    assert!(
        app.invocation_view().expect("stable picker").1[0]
            .qualifier
            .ends_with("working")
    );

    app.close_invocation_picker();
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    assert!(
        app.invocation_view().expect("reopened picker").1[0]
            .qualifier
            .ends_with("idle")
    );
}

#[test]
fn completion_enforces_the_canonical_live_result_bound() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    let effects = app.open_invocation_picker();
    let references = (0..70)
        .map(|index| {
            reference(
                &format!("reviewer-{index}"),
                ("w2", Some("Product")),
                ("w2:t4", Some("Review")),
                &format!("w2:p{index}"),
                AgentState::Idle,
            )
        })
        .collect();
    complete_live(&mut app, &effects, Ok(references));

    assert_eq!(
        app.invocation_view().expect("bounded picker").1.len(),
        crate::ports::invocation::MAX_INVOCATION_REFERENCES
    );
}

#[test]
fn full_live_catalog_does_not_crowd_out_installed_invocations() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(
        &mut app,
        cwd.path(),
        vec![
            entry(
                "$skill",
                crate::ports::invocation::InvocationKind::Skill,
                crate::ports::invocation::InvocationScope::Project,
            ),
            entry(
                "$agent",
                crate::ports::invocation::InvocationKind::Agent,
                crate::ports::invocation::InvocationScope::Global,
            ),
            entry(
                "$plugin",
                crate::ports::invocation::InvocationKind::Command,
                crate::ports::invocation::InvocationScope::Plugin,
            ),
        ],
    );
    let live = (0..crate::ports::invocation::MAX_INVOCATION_REFERENCES)
        .map(|index| {
            reference(
                &format!("reviewer-{index}"),
                ("w2", Some("Product")),
                ("w2:t4", Some("Review")),
                &format!("w2:p{index}"),
                AgentState::Idle,
            )
        })
        .collect();
    open_with_live(&mut app, live);

    let choices = app.invocation_view().expect("picker").1;
    for token in ["$skill", "$agent", "$plugin"] {
        assert!(choices.iter().any(|choice| choice.token == token));
    }
    assert_eq!(
        choices
            .iter()
            .filter(|choice| choice.group.as_deref() == Some("Live in Herdr"))
            .count(),
        1
    );
    assert_eq!(
        choices.len(),
        3 + crate::ports::invocation::MAX_INVOCATION_REFERENCES
    );
}
