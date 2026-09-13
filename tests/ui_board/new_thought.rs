//! Commands and pointer regressions for the canonical new-thought intention.

use super::{
    Fixture, HitTarget, KeyStroke, LogicalKey, PointerButton, PointerInput, PointerKind, Rect,
    UiInput, UiKey, UiSettings,
};

use proqi::{
    application::{Effect, InteractionMode},
    domain::ThoughtId,
    ports::environment::IdGenerator,
};

#[derive(Clone, Copy)]
enum CommandActivation {
    Keyboard,
    Mouse,
}

fn query_new_thought_command(fixture: &mut Fixture) {
    for character in "new thought".chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    let (_, entries, selected) = fixture.app.palette_view().expect("commands overlay");
    assert_eq!(entries, vec!["New thought"]);
    assert_eq!(selected, 0);
}

fn open_new_thought_command(fixture: &mut Fixture) {
    let area = Rect::new(0, 0, 48, 10);
    let _terminal = super::draw(fixture, area.width, area.height);
    let commands = fixture
        .app
        .prepare_frame(area)
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == HitTarget::Commands).then_some(area))
        .expect("commands control");
    fixture.pointer(
        commands.x,
        commands.y,
        PointerKind::Down(PointerButton::Left),
    );
    query_new_thought_command(fixture);
}

fn execute_new_thought_command(
    fixture: &mut Fixture,
    activation: CommandActivation,
) -> Vec<Effect> {
    open_new_thought_command(fixture);
    match activation {
        CommandActivation::Keyboard => fixture.effects(crate::key_input(UiKey::Enter)),
        CommandActivation::Mouse => {
            let area = Rect::new(0, 0, 48, 10);
            let _terminal = super::draw(fixture, area.width, area.height);
            let item = fixture
                .app
                .prepare_frame(area)
                .overlay
                .expect("commands geometry")
                .items[0];
            fixture.effects(UiInput::Pointer(PointerInput {
                column: item.x,
                row: item.y,
                kind: PointerKind::Down(PointerButton::Left),
                extend_selection: false,
            }))
        }
    }
}

fn thought_ids(fixture: &Fixture) -> Vec<ThoughtId> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.id)
        .collect()
}

#[test]
fn commands_new_from_explicit_empty_board_enters_provisional_compose() {
    for activation in [CommandActivation::Keyboard, CommandActivation::Mouse] {
        let mut fixture = Fixture::new();
        fixture.input(crate::key_input(UiKey::Escape));
        let sequence = fixture.app.state.board.session.last_durable_sequence;
        let history = fixture.app.state.board_history().to_vec();
        let mut expected_ids = fixture.ids.clone();

        let effects = execute_new_thought_command(&mut fixture, activation);

        assert!(effects.is_empty());
        assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
        assert!(fixture.app.compose_editor_visible());
        assert!(fixture.app.state.board.live_thoughts().is_empty());
        assert_eq!(fixture.app.state.board_history(), history.as_slice());
        assert_eq!(
            fixture.app.state.board.session.last_durable_sequence,
            sequence
        );
        assert_eq!(fixture.app.editor_snapshot().expect("compose").content, "");
        assert_eq!(fixture.ids.thought_id(), expected_ids.thought_id());
        assert_eq!(fixture.ids.operation_id(), expected_ids.operation_id());
    }
}

#[test]
fn commands_new_from_empty_compose_remains_provisional() {
    for activation in [CommandActivation::Keyboard, CommandActivation::Mouse] {
        let settings = UiSettings {
            shortcuts: proqi::ui::ShortcutRegistry::from_toml(
                "schema_version=1\n[bindings.compose]\n\"commands.open\"=[{key='F6'}]",
            )
            .expect("Compose Commands binding"),
            ..UiSettings::default()
        };
        let mut fixture = Fixture::with_settings(settings);
        let sequence = fixture.app.state.board.session.last_durable_sequence;
        let mut expected_ids = fixture.ids.clone();

        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
            6,
        ))));
        query_new_thought_command(&mut fixture);
        let effects = match activation {
            CommandActivation::Keyboard => fixture.effects(crate::key_input(UiKey::Enter)),
            CommandActivation::Mouse => {
                let area = Rect::new(0, 0, 48, 10);
                let _terminal = super::draw(&mut fixture, area.width, area.height);
                let item = fixture
                    .app
                    .prepare_frame(area)
                    .overlay
                    .expect("commands geometry")
                    .items[0];
                fixture.effects(UiInput::Pointer(PointerInput {
                    column: item.x,
                    row: item.y,
                    kind: PointerKind::Down(PointerButton::Left),
                    extend_selection: false,
                }))
            }
        };

        assert!(effects.is_empty());
        assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
        assert!(fixture.app.state.board.live_thoughts().is_empty());
        assert!(fixture.app.state.board_history().is_empty());
        assert_eq!(
            fixture.app.state.board.session.last_durable_sequence,
            sequence
        );
        assert_eq!(fixture.app.editor_snapshot().expect("compose").content, "");
        assert_eq!(fixture.ids.thought_id(), expected_ids.thought_id());
        assert_eq!(fixture.ids.operation_id(), expected_ids.operation_id());
    }
}

#[test]
fn commands_new_at_final_insertion_row_appends_after_non_tail_insertion() {
    for activation in [CommandActivation::Keyboard, CommandActivation::Mouse] {
        let mut fixture = Fixture::new();
        for content in ["first", "middle", "last"] {
            super::navigation::durable_thought(&mut fixture, content);
        }
        for _ in 0..4 {
            fixture.input(crate::key_input(UiKey::Character('k')));
        }
        fixture.input(crate::key_input(UiKey::Escape));

        let count = fixture.app.state.board.live_thoughts().len();
        for _ in 0..count {
            fixture.input(crate::key_input(UiKey::Character('j')));
        }
        assert!(fixture.app.insertion_focused());
        let expected_prefix = thought_ids(&fixture);
        assert!(fixture.app.state.insertion_index < expected_prefix.len());

        let effects = execute_new_thought_command(&mut fixture, activation);

        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
                .count(),
            1
        );
        let created = fixture.app.active_thought_id().expect("new tail editor");
        let order = thought_ids(&fixture);
        assert_eq!(&order[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(order.last(), Some(&created));
        assert_eq!(
            fixture.app.state.board.thought(created).unwrap().content,
            ""
        );
        assert_eq!(
            fixture.app.interaction_mode(),
            InteractionMode::Edit {
                thought_id: created
            }
        );
    }
}

#[test]
fn pointer_new_at_final_row_ignores_a_stale_non_tail_insertion_index() {
    let mut fixture = Fixture::new();
    for content in ["first", "middle", "last"] {
        super::navigation::durable_thought(&mut fixture, content);
    }
    for _ in 0..4 {
        fixture.input(crate::key_input(UiKey::Character('k')));
    }
    fixture.input(crate::key_input(UiKey::Escape));
    let expected_prefix = thought_ids(&fixture);
    assert!(!fixture.app.insertion_focused());
    assert!(fixture.app.state.insertion_index < expected_prefix.len());

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 48, 20));
    let insert = layout.insert.expect("visible final insertion row");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: insert.x,
        row: insert.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));

    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
            .count(),
        1
    );
    let created = fixture.app.active_thought_id().expect("new tail editor");
    let order = thought_ids(&fixture);
    assert_eq!(&order[..expected_prefix.len()], expected_prefix.as_slice());
    assert_eq!(order.last(), Some(&created));
    assert_eq!(
        fixture.app.state.board.thought(created).unwrap().content,
        ""
    );
    assert_eq!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit {
            thought_id: created
        }
    );
}
