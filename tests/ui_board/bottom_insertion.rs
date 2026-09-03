//! Final virtual-row insertion ordering across navigation, layout, and recovery state.

use super::{
    Fixture, PointerButton, PointerKind, Rect, UiInput, UiKey,
    navigation::{durable_thought, visual},
};

use proqi::{
    application::{DurabilityState, Effect, InteractionMode},
    domain::{OperationSequence, ThoughtId},
    ports::editor::CursorMovement,
};

fn seeded_long_board() -> Fixture {
    let mut fixture = Fixture::new();
    for index in 0..10 {
        durable_thought(
            &mut fixture,
            &format!(
                "thought {index}: Grüße 界 👩‍💻\tcontrol\u{7} with enough words to wrap in a narrow pane"
            ),
        );
    }
    fixture.input(UiInput::Key(UiKey::Character('c')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    fixture
}

fn insert_at_top(fixture: &mut Fixture) -> (ThoughtId, Vec<Effect>) {
    let live = fixture.app.state.board.live_thoughts();
    let current = fixture
        .app
        .state
        .focused_thought
        .and_then(|focused| live.iter().position(|thought| thought.id == focused))
        .expect("focused setup thought");
    for _ in 0..current {
        fixture.input(UiInput::Key(UiKey::Character('k')));
    }
    fixture.input(visual(CursorMovement::VisualUp, false));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('k')));
    let thought_id = fixture.app.active_thought_id().expect("top blank editor");
    assert_eq!(fixture.app.state.board.live_thoughts()[0].id, thought_id);
    (thought_id, effects)
}

fn focus_bottom_insertion(fixture: &mut Fixture) {
    let count = fixture.app.state.board.live_thoughts().len();
    for step in 0..count {
        let input = if step % 2 == 0 {
            visual(CursorMovement::VisualDown, false)
        } else {
            UiInput::Key(UiKey::Character('j'))
        };
        fixture.input(input);
    }
    assert!(fixture.app.insertion_focused());
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

fn acknowledge_pending(fixture: &mut Fixture) {
    loop {
        let next = match fixture.app.state.durability {
            DurabilityState::Durable { .. } => return,
            DurabilityState::Pending { durable, .. } => {
                durable.checked_next().expect("next pending sequence")
            }
            DurabilityState::Failed { .. } => panic!("setup persistence failed"),
        };
        assert!(fixture.app.acknowledge_persistence(next, true).is_empty());
    }
}

fn paste_native_clipboard_at_bottom(fixture: &mut Fixture) {
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    fixture.app.complete_clipboard_read(
        *request_id,
        Ok("native bottom paste".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );
}

fn effect_sequence(effects: &[Effect]) -> OperationSequence {
    effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("one persistence effect")
}

#[test]
fn bottom_confirmation_appends_after_top_insertion_across_layout_and_key_spellings() {
    let cases = [
        (Rect::new(0, 0, 24, 6), 0, false),
        (Rect::new(0, 0, 34, 9), 4, true),
        (Rect::new(0, 0, 72, 12), 16, false),
    ];
    for (area, scroll_steps, first_is_vim) in cases {
        let mut fixture = seeded_long_board();
        let (_top, _effects) = insert_at_top(&mut fixture);
        fixture.input(UiInput::Key(UiKey::Escape));
        let expected_prefix = thought_ids(&fixture);

        for resize in [Rect::new(0, 0, 18, 7), Rect::new(0, 0, 80, 18), area] {
            let _layout = fixture.app.prepare_frame(resize);
        }
        for _ in 0..scroll_steps {
            let _layout = fixture.app.prepare_frame(area);
            fixture.pointer(1, 1, PointerKind::ScrollDown);
        }
        focus_bottom_insertion(&mut fixture);

        let first = if first_is_vim {
            UiInput::Key(UiKey::Character('j'))
        } else {
            visual(CursorMovement::VisualDown, false)
        };
        let second = if first_is_vim {
            visual(CursorMovement::VisualDown, false)
        } else {
            UiInput::Key(UiKey::Character('j'))
        };
        assert!(fixture.effects(first).is_empty());
        let effects = fixture.effects(second);
        assert_eq!(effects.len(), 1);
        let first_bottom = fixture.app.active_thought_id().expect("bottom editor");
        let order = thought_ids(&fixture);
        assert_eq!(&order[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(order.last(), Some(&first_bottom));
        assert!(matches!(
            fixture.app.interaction_mode(),
            InteractionMode::Edit { thought_id } if thought_id == first_bottom
        ));

        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(visual(CursorMovement::VisualDown, false));
        fixture.input(UiInput::Key(UiKey::Character('j')));
        fixture.input(visual(CursorMovement::VisualDown, false));
        let second_bottom = fixture
            .app
            .active_thought_id()
            .expect("repeated bottom editor");
        assert_eq!(thought_ids(&fixture).last(), Some(&second_bottom));

        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Undo));
        assert_eq!(thought_ids(&fixture).last(), Some(&first_bottom));
        fixture.input(UiInput::Key(UiKey::Redo));
        assert_eq!(thought_ids(&fixture).last(), Some(&second_bottom));
    }
}

#[test]
fn final_insertion_row_commands_pointer_and_paste_all_append() {
    for path in 0..5 {
        let mut fixture = seeded_long_board();
        let (_top, _effects) = insert_at_top(&mut fixture);
        fixture.input(UiInput::Key(UiKey::Escape));
        let expected_prefix = thought_ids(&fixture);
        focus_bottom_insertion(&mut fixture);

        match path {
            0 => fixture.input(UiInput::Key(UiKey::Enter)),
            1 => fixture.input(UiInput::Key(UiKey::Character('n'))),
            _ => {
                if path == 2 {
                    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 30, 7));
                    let insert = layout.insert.expect("visible final insertion row");
                    fixture.pointer(insert.x, insert.y, PointerKind::Down(PointerButton::Left));
                } else if path == 3 {
                    fixture.input(UiInput::Paste("bracketed bottom paste".to_owned()));
                } else {
                    paste_native_clipboard_at_bottom(&mut fixture);
                }
            }
        }

        let created = fixture.app.active_thought_id().expect("new bottom editor");
        let order = thought_ids(&fixture);
        assert_eq!(&order[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(order.last(), Some(&created));
    }
}

#[test]
fn failed_bottom_append_is_retryable_without_partial_or_misordered_state() {
    let mut fixture = seeded_long_board();
    acknowledge_pending(&mut fixture);
    let (_top, top_effects) = insert_at_top(&mut fixture);
    fixture
        .app
        .acknowledge_persistence(effect_sequence(&top_effects), true);
    fixture.input(UiInput::Key(UiKey::Escape));
    focus_bottom_insertion(&mut fixture);
    let expected_prefix = thought_ids(&fixture);
    fixture.input(visual(CursorMovement::VisualDown, false));
    let bottom_effects = fixture.effects(UiInput::Key(UiKey::Character('j')));
    let failed = effect_sequence(&bottom_effects);
    let created = fixture.app.active_thought_id().expect("bottom editor");
    assert_eq!(thought_ids(&fixture).last(), Some(&created));

    fixture.app.acknowledge_persistence(failed, false);
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence: failed }]
    );
    fixture.app.acknowledge_persistence(failed, true);
    let order = thought_ids(&fixture);
    assert_eq!(&order[..expected_prefix.len()], expected_prefix.as_slice());
    assert_eq!(order.last(), Some(&created));
    assert_eq!(
        order.iter().filter(|thought| **thought == created).count(),
        1
    );
}
