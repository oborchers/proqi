//! Herdr invocation-reference snapshot scenarios and fixtures.

use proqi::{
    application::Effect,
    ports::{
        agent::{AgentState, CODEX_AGENT_KIND, HarnessKind},
        invocation::{
            InvocationReferenceDiscovery, InvocationReferenceProvider, LiveAgentReference,
        },
    },
    ui::{ThemePreference, UiInput, UiKey},
};

use super::{Fixture, assert_platform_snapshot, snapshot};

#[test]
fn existing_invocation_command_opens_terminal_independent_live_reference_picker() {
    let mut fixture = Fixture::new();
    fixture.paste("Coordinate with another agent");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "Insert discovered invocation".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    complete_live_reference(&mut fixture, &effects);

    assert_platform_snapshot!(
        "existing_invocation_command_opens_terminal_independent_live_reference_picker",
        snapshot(&mut fixture, 72, 12, ThemePreference::Dark)
    );
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_platform_snapshot!(
        "inline_herdr_reference",
        snapshot(&mut fixture, 72, 8, ThemePreference::Dark)
    );
}

pub(super) fn complete_live_reference(fixture: &mut Fixture, effects: &[Effect]) {
    let [Effect::DiscoverInvocationReferences(request)] = effects else {
        panic!("live reference refresh effect");
    };
    let reference = LiveAgentReference::new(
        InvocationReferenceProvider::Herdr,
        Some("reviewer".to_owned()),
        HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        "w2".to_owned(),
        Some("Product".to_owned()),
        "w2:t4".to_owned(),
        Some("Review".to_owned()),
        "w2:p9".to_owned(),
        AgentState::Working,
    )
    .expect("live reference");
    fixture
        .app
        .complete_invocation_reference_discovery(InvocationReferenceDiscovery {
            generation: request.generation,
            references: Ok(vec![reference]),
        });
}
