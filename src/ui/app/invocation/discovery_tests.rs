use super::tests::contract::{app, entry};
use crate::{
    application::Effect,
    ports::invocation::{
        InvocationCompleteness, InvocationDiscovery, InvocationIncompleteReason, InvocationKind,
        InvocationScope,
    },
};

#[test]
fn stale_complete_and_incomplete_results_cannot_replace_the_newest_generation_or_cwd() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("other tempdir");
    let (mut app, _, _) = app("$pl", cwd.path());
    let first = app.refresh_invocations();
    let second = app.refresh_invocations();
    let [Effect::DiscoverInvocations(old)] = first.as_slice() else {
        panic!("first refresh");
    };
    let [Effect::DiscoverInvocations(newest)] = second.as_slice() else {
        panic!("second refresh");
    };
    assert!(app.complete_invocation_discovery(InvocationDiscovery {
        generation: newest.generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: vec![entry(
            "$new",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
        completeness: InvocationCompleteness::Complete,
    }));
    let mut incomplete = InvocationCompleteness::Complete;
    incomplete.add(InvocationIncompleteReason::EntryBudget {
        observed: 2_049,
        limit: 2_048,
    });
    assert!(!app.complete_invocation_discovery(InvocationDiscovery {
        generation: old.generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: vec![entry(
            "$old",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
        completeness: incomplete,
    }));
    assert!(!app.complete_invocation_discovery(InvocationDiscovery {
        generation: newest.generation,
        cwd: other.path().to_owned(),
        global: Vec::new(),
        project: vec![entry(
            "$wrong",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
        completeness: InvocationCompleteness::Complete,
    }));

    app.close_invocation_picker();
    app.open_invocation_picker();
    let choices = app.invocation_view().expect("manual picker").1;
    assert!(choices.iter().any(|choice| choice.token == "$new"));
    assert!(!choices.iter().any(|choice| choice.token == "$old"));
    assert_eq!(app.invocation_notice(), None);
}
