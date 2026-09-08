//! Terminal-safe boundary navigation and direct relative insertion behavior.

use super::{
    Effect, Fixture, KeyStroke, LogicalKey, LogicalModifiers, Rect, UiInput, UiKey,
    navigation::durable_thought,
};
use proqi::{
    application::{DurabilityState, InteractionMode},
    domain::{OperationSequence, TextPosition, ThoughtPresentation},
    ports::{
        editor::{CursorMovement, TextSelection},
        environment::IdGenerator,
    },
    ui::{ShortcutActionId as Shortcut, UiSettings},
};

fn shortcut(action: Shortcut) -> UiInput {
    let (key, modifiers) = match action {
        Shortcut::FocusFirst => (LogicalKey::Up, LogicalModifiers::CONTROL),
        Shortcut::FocusLast => (LogicalKey::Down, LogicalModifiers::CONTROL),
        Shortcut::ExtendFirst => (
            LogicalKey::Up,
            LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT),
        ),
        Shortcut::ExtendLast => (
            LogicalKey::Down,
            LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT),
        ),
        Shortcut::InsertAbove => (LogicalKey::Up, LogicalModifiers::ALT),
        Shortcut::InsertBelow => (LogicalKey::Down, LogicalModifiers::ALT),
        _ => unreachable!("test helper only covers terminal-safe boundary actions"),
    };
    UiInput::KeyStroke(KeyStroke::press(key).with_modifiers(modifiers))
}

fn contents(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect()
}

fn focused_content(fixture: &Fixture) -> Option<&str> {
    let focused = fixture.app.state.focused_thought?;
    fixture
        .app
        .state
        .board
        .thought(focused)
        .map(|thought| thought.content.as_str())
}

fn selected_contents(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| fixture.app.thought_selected(thought.id))
        .map(|thought| thought.content.as_str())
        .collect()
}

fn effect_sequence(effects: &[Effect]) -> OperationSequence {
    effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("one persistence sequence")
}

#[test]
fn first_and_last_clamp_to_live_thoughts_across_selection_layout_and_collapse() {
    let mut empty = Fixture::new();
    empty.input(super::key_input(UiKey::Escape));
    empty.input(shortcut(Shortcut::FocusFirst));
    empty.input(shortcut(Shortcut::FocusLast));
    assert!(empty.app.state.board.live_thoughts().is_empty());
    assert!(empty.app.insertion_focused());

    let mut fixture = Fixture::new();
    for content in ["first", "Grüße 界", "third", "fourth", "last 👩‍💻"] {
        durable_thought(&mut fixture, content);
    }
    fixture.input(super::key_input(UiKey::Character(' ')));
    fixture.input(shortcut(Shortcut::FocusFirst));
    fixture.input(shortcut(Shortcut::FocusFirst));
    assert_eq!(focused_content(&fixture), Some("first"));
    assert_eq!(selected_contents(&fixture), ["last 👩‍💻"]);

    fixture.input(super::key_input(UiKey::Escape));
    fixture.input(super::key_input(UiKey::Character('c')));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        ThoughtPresentation::Collapsed,
    );
    for area in [Rect::new(0, 0, 18, 5), Rect::new(0, 0, 80, 20)] {
        let _layout = fixture.app.prepare_frame(area);
    }
    fixture.input(shortcut(Shortcut::FocusLast));
    fixture.input(shortcut(Shortcut::FocusLast));
    assert_eq!(focused_content(&fixture), Some("last 👩‍💻"));
    assert!(!fixture.app.insertion_focused());

    fixture.input(super::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert!(fixture.app.insertion_focused());
    fixture.input(shortcut(Shortcut::FocusFirst));
    assert_eq!(focused_content(&fixture), Some("first"));
    assert!(!fixture.app.insertion_focused());
}

#[test]
#[cfg(target_os = "macos")]
fn boundary_range_extension_keeps_one_anchor_and_never_selects_insertion() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third", "fourth", "fifth"] {
        durable_thought(&mut fixture, content);
    }
    fixture.input(super::key_input(UiKey::Character('k')));
    fixture.input(super::key_input(UiKey::Character('k')));
    assert_eq!(focused_content(&fixture), Some("third"));

    fixture.input(shortcut(Shortcut::ExtendFirst));
    assert_eq!(selected_contents(&fixture), ["first", "second", "third"]);
    assert_eq!(focused_content(&fixture), Some("first"));

    fixture.input(shortcut(Shortcut::ExtendLast));
    fixture.input(shortcut(Shortcut::ExtendLast));
    assert_eq!(selected_contents(&fixture), ["third", "fourth", "fifth"]);
    assert_eq!(focused_content(&fixture), Some("fifth"));
    assert!(!fixture.app.insertion_focused());
}

#[test]
fn range_latch_turns_first_and_last_focus_actions_into_boundary_extension() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third", "fourth", "fifth"] {
        durable_thought(&mut fixture, content);
    }
    fixture.input(super::key_input(UiKey::Character('k')));
    fixture.input(super::key_input(UiKey::Character('k')));
    fixture.input(super::key_input(UiKey::Character('v')));

    fixture.input(shortcut(Shortcut::FocusFirst));
    assert_eq!(selected_contents(&fixture), ["first", "second", "third"]);
    assert_eq!(focused_content(&fixture), Some("first"));

    fixture.input(shortcut(Shortcut::FocusLast));
    assert_eq!(selected_contents(&fixture), ["third", "fourth", "fifth"]);
    assert_eq!(focused_content(&fixture), Some("fifth"));
    assert!(!fixture.app.insertion_focused());
}

#[test]
fn direct_insert_uses_focused_reference_and_clears_every_selection_shape() {
    for (focus_steps, action, expected_index) in [
        (0, Shortcut::InsertAbove, 0),
        (1, Shortcut::InsertAbove, 1),
        (1, Shortcut::InsertBelow, 2),
        (2, Shortcut::InsertBelow, 3),
    ] {
        let mut fixture = Fixture::new();
        for content in ["same", "Grüße 界", "same"] {
            durable_thought(&mut fixture, content);
        }
        fixture.input(super::key_input(UiKey::Character(' ')));
        fixture.input(shortcut(Shortcut::FocusFirst));
        for _ in 0..focus_steps {
            fixture.input(super::key_input(UiKey::Character('j')));
        }
        fixture.input(super::key_input(UiKey::Character(' ')));

        let effects = fixture.effects(shortcut(action));

        assert!(matches!(
            effects.as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 4);
        assert_eq!(contents(&fixture)[expected_index], "");
        assert_eq!(
            fixture.app.editor_snapshot().expect("blank editor").content,
            ""
        );
        assert!(matches!(
            fixture.app.interaction_mode(),
            InteractionMode::Edit { .. }
        ));
        assert!(selected_contents(&fixture).is_empty());
        assert_eq!(
            fixture.app.active_thought_id(),
            Some(fixture.app.state.board.live_thoughts()[expected_index].id),
        );
    }
}

#[test]
fn direct_insert_uses_the_range_endpoint_and_clears_the_latch() {
    for (action, expected) in [
        (
            Shortcut::InsertAbove,
            ["first", "second", "", "third", "fourth"],
        ),
        (
            Shortcut::InsertBelow,
            ["first", "second", "third", "", "fourth"],
        ),
    ] {
        let mut fixture = Fixture::new();
        for content in ["first", "second", "third", "fourth"] {
            durable_thought(&mut fixture, content);
        }
        fixture.input(shortcut(Shortcut::FocusFirst));
        fixture.input(super::key_input(UiKey::Character('j')));
        fixture.input(super::key_input(UiKey::Character('v')));
        fixture.input(super::key_input(UiKey::Character('j')));
        assert_eq!(selected_contents(&fixture), ["second", "third"]);

        let effects = fixture.effects(shortcut(action));

        assert!(matches!(
            effects.as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        assert_eq!(contents(&fixture), expected);
        assert!(selected_contents(&fixture).is_empty());
        assert_eq!(
            fixture
                .app
                .editor_snapshot()
                .expect("created editor")
                .content,
            ""
        );
        fixture.input(super::key_input(UiKey::Escape));
        fixture.input(shortcut(Shortcut::FocusFirst));
        assert!(
            selected_contents(&fixture).is_empty(),
            "range latch must clear"
        );
        assert_eq!(focused_content(&fixture), Some("first"));
    }
}

#[test]
fn relative_insert_is_one_retryable_undoable_board_operation() {
    let mut fixture = Fixture::new();
    for content in ["first", "middle", "last"] {
        durable_thought(&mut fixture, content);
    }
    while let DurabilityState::Pending { durable, .. } = fixture.app.state.durability {
        fixture.app.acknowledge_persistence(
            durable.checked_next().expect("pending setup sequence"),
            true,
        );
    }
    fixture.input(super::key_input(UiKey::Character('k')));
    let before = contents(&fixture)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let effects = fixture.effects(shortcut(Shortcut::InsertBelow));
    let sequence = effect_sequence(&effects);
    let created = fixture.app.active_thought_id().expect("created blank");
    fixture.app.acknowledge_persistence(sequence, false);
    assert_eq!(
        fixture.effects(super::key_input(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }],
    );
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(super::key_input(UiKey::Escape));

    fixture.input(super::key_input(UiKey::Undo));
    assert_eq!(
        contents(&fixture),
        before.iter().map(String::as_str).collect::<Vec<_>>()
    );
    fixture.input(super::key_input(UiKey::Redo));
    assert!(fixture.app.state.board.thought(created).is_some());
    assert_eq!(fixture.app.state.board.live_thoughts()[2].id, created);
}

#[test]
fn empty_insertion_locked_and_stale_references_never_partially_insert() {
    let mut empty = Fixture::new();
    for action in [Shortcut::InsertAbove, Shortcut::InsertBelow] {
        assert!(empty.effects(shortcut(action)).is_empty());
    }
    assert!(empty.app.state.board.live_thoughts().is_empty());

    let mut stale = Fixture::new();
    durable_thought(&mut stale, "still present");
    stale.app.state.focused_thought = Some(stale.ids.thought_id());
    assert!(stale.effects(shortcut(Shortcut::InsertAbove)).is_empty());
    assert_eq!(contents(&stale), ["still present"]);
    assert_eq!(
        stale.app.status_text(),
        Some("focused thought is no longer available"),
    );

    let mut locked = Fixture::new();
    super::agent::prepare_thought(&mut locked);
    let target = super::agent::target(proqi::domain::Direction::Right, "w1:p2");
    locked.app.complete_agent_discovery(Ok(vec![target]));
    let submission = locked.effects(super::key_input(UiKey::Character('s')));
    let _request = super::agent::start_submission(&mut locked, &submission);
    assert!(locked.effects(shortcut(Shortcut::InsertBelow)).is_empty());
    assert_eq!(contents(&locked), ["exact prompt\nGrüße 第二行"]);
    assert_eq!(
        locked.app.status_text(),
        Some("focused thought has a submission in progress"),
    );
}

#[test]
fn logical_control_moves_and_extends_over_exact_complex_thought_bytes() {
    let mut fixture = Fixture::new();
    let content = "start\r\n\t界e\u{301}👩‍💻\u{7}\nend";
    let create = fixture.paste(content);
    fixture.app.acknowledge_persistence(create, true);
    let control = LogicalModifiers::CONTROL;
    let control_shift = control.union(LogicalModifiers::SHIFT);

    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Up).with_modifiers(control),
    ));
    assert_eq!(
        fixture.app.editor_snapshot().expect("start").cursor,
        TextPosition::new(0, 0),
    );
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Down).with_modifiers(control),
    ));
    assert_eq!(
        fixture.app.editor_snapshot().expect("end").cursor,
        TextPosition::new(2, 3),
    );

    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Up).with_modifiers(control),
    ));
    for _ in 0..2 {
        fixture.input(super::key_input(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Down).with_modifiers(control_shift),
    ));
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("forward selection")
            .selection,
        Some(TextSelection {
            start: TextPosition::new(0, 2),
            end: TextPosition::new(2, 3),
        }),
    );
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Up).with_modifiers(control_shift),
    ));
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("reverse selection")
            .selection,
        Some(TextSelection {
            start: TextPosition::new(0, 0),
            end: TextPosition::new(0, 2),
        }),
    );
}

#[test]
fn boundary_move_flushes_pending_edit_and_recovery_retries_exactly_once() {
    let mut fixture = Fixture::new();
    let create = fixture.paste("pending Grüße");
    fixture.app.acknowledge_persistence(create, true);
    fixture.input(super::key_input(UiKey::Character('!')));

    let effects = fixture.effects(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Up).with_modifiers(LogicalModifiers::CONTROL),
    ));
    let sequence = effect_sequence(&effects);
    assert_eq!(
        fixture.app.editor_snapshot().expect("moved editor").cursor,
        TextPosition::new(0, 0),
    );
    fixture.app.acknowledge_persistence(sequence, false);
    assert_eq!(
        fixture.effects(super::key_input(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }],
    );
    fixture.app.acknowledge_persistence(sequence, true);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("recovered editor")
            .content,
        "pending Grüße!",
    );
}

#[test]
fn new_commands_execute_by_keyboard_and_mouse_with_thought_availability() {
    let mut keyboard = Fixture::new();
    for content in ["first", "second"] {
        durable_thought(&mut keyboard, content);
    }
    keyboard.input(super::key_input(UiKey::Character(':')));
    for character in "insert thought above".chars() {
        keyboard.input(super::key_input(UiKey::Character(character)));
    }
    let (_, entries, selected) = keyboard.app.palette_view().expect("keyboard palette");
    assert_eq!(entries, ["Insert thought above"]);
    assert_eq!(selected, 0);
    keyboard.input(super::key_input(UiKey::Enter));
    assert_eq!(contents(&keyboard), ["first", "", "second"]);

    keyboard.input(super::key_input(UiKey::Escape));
    keyboard.input(super::key_input(UiKey::Character(':')));
    for character in "go to first thought".chars() {
        keyboard.input(super::key_input(UiKey::Character(character)));
    }
    let area = keyboard
        .app
        .prepare_frame(Rect::new(0, 0, 48, 10))
        .overlay
        .expect("mouse palette")
        .items[0];
    keyboard.pointer(
        area.x,
        area.y,
        super::PointerKind::Down(super::PointerButton::Left),
    );
    assert_eq!(focused_content(&keyboard), Some("first"));
}

#[test]
fn unavailable_board_command_binding_does_not_mutate_after_opening_commands() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"commands.open\"=[{key='F6'}]\n[bindings.commands]\n\"thought.insert_above\"=[{key='F5'}]",
        )
        .expect("safe Commands binding"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    durable_thought(&mut fixture, "old");
    fixture.input(super::key_input(UiKey::Enter));
    fixture.input(super::key_input(UiKey::Character('!')));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        6,
    ))));

    let effects = fixture.effects(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        5,
    ))));

    assert!(effects.is_empty());
    assert_eq!(contents(&fixture), ["old!"]);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("pending editor")
            .content,
        "old!",
    );
    assert_eq!(
        fixture.app.status_text(),
        Some("command is unavailable in the current state"),
    );
}
