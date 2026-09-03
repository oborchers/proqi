use super::tests::contract::{app, entry, install};
use crate::{
    ports::invocation::{InvocationKind, InvocationScope},
    ui::{FastNavigation, UiInput, UiKey},
};

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
