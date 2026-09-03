use super::tests::contract::{app, entry, install};
use crate::{
    application::Effect,
    ports::invocation::{
        InvocationCompleteness, InvocationDiscovery, InvocationIncompleteReason, InvocationKind,
        InvocationScope,
    },
    ui::{FastNavigation, Theme, ThemePreference, UiInput, UiKey, render},
};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

#[test]
fn picker_pages_five_choices_across_structural_groups_and_replacement() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("$s", cwd.path());
    let entries = (0..9)
        .map(|index| {
            entry(
                &format!("$skill{index}"),
                InvocationKind::Skill,
                if index < 4 {
                    InvocationScope::Project
                } else {
                    InvocationScope::Global
                },
            )
        })
        .collect::<Vec<_>>();
    install(&mut app, cwd.path(), entries);
    app.refresh_invocation_popup();
    let expected = app.invocation_view().expect("picker").1[5].token.clone();
    app.handle(
        UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    let (_, visible, selected) = app.invocation_view().expect("paged picker");
    assert_eq!(visible[selected].token, expected);

    install(
        &mut app,
        cwd.path(),
        vec![entry(
            "$single",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
    );
    let (_, visible, selected) = app.invocation_view().expect("replaced picker");
    assert_eq!(selected, 0);
    assert_eq!(visible[0].token, "$single");
}

#[test]
fn more_than_twenty_matches_are_navigable_and_explicitly_presented() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("$s", cwd.path());
    let entries = (0..25)
        .map(|index| {
            entry(
                &format!("$skill{index:02}"),
                InvocationKind::Skill,
                InvocationScope::Project,
            )
        })
        .collect();
    install(&mut app, cwd.path(), entries);
    app.refresh_invocation_popup();

    assert_eq!(app.invocation_match_count(), 25);
    assert_eq!(
        app.invocation_notice(),
        Some(" more results exist, refine query ")
    );
    for _ in 0..22 {
        app.handle(UiInput::Key(UiKey::PickerNext), &mut ids, &clock);
    }
    let (_, visible, selected) = app.invocation_view().expect("paged picker");
    assert_eq!(visible[selected].token, "$skill22");
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "$skill22 ");
}

#[test]
fn more_than_twenty_matches_have_a_visible_truthful_snapshot() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("$s", cwd.path());
    install(
        &mut app,
        cwd.path(),
        (0..25)
            .map(|index| {
                entry(
                    &format!("$skill{index:02}"),
                    InvocationKind::Skill,
                    InvocationScope::Project,
                )
            })
            .collect(),
    );
    app.refresh_invocation_popup();
    insta::assert_snapshot!("invocation_more_results", picker_snapshot(&mut app));
}

#[test]
fn incomplete_discovery_has_a_visible_truthful_snapshot() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("$s", cwd.path());
    let effects = app.refresh_invocations();
    let [Effect::DiscoverInvocations(request)] = effects.as_slice() else {
        panic!("invocation refresh");
    };
    let mut completeness = InvocationCompleteness::Complete;
    completeness.add(InvocationIncompleteReason::RecursiveDepth {
        observed: 7,
        limit: 6,
    });
    app.complete_invocation_discovery(InvocationDiscovery {
        generation: request.generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: vec![entry(
            "$skill",
            InvocationKind::Skill,
            InvocationScope::Project,
        )],
        completeness,
    });
    app.refresh_invocation_popup();
    insta::assert_snapshot!("invocation_incomplete_results", picker_snapshot(&mut app));
}

fn picker_snapshot(app: &mut crate::ui::BoardApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(56, 10)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
