use ratatui_core::layout::Rect;

use super::behavior::{app_with_thought, candidate, created, next_commit};
use crate::{
    application::{Effect, InteractionMode},
    domain::Timestamp,
    ui::{PastePayload, PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};

#[test]
fn commit_barrier_replays_keyboard_plain_and_annotated_paste_in_order() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.state.mode = InteractionMode::Edit {
        thought_id: original_id,
    };
    app.sync_editor_from_state();
    app.queue_screenshot_candidates([candidate(31)]);
    let capture = next_commit(&mut app, &mut ids, &clock);

    for input in [
        UiInput::Key(UiKey::Character('1')),
        UiInput::Paste("two".to_owned()),
        UiInput::PasteAnnotated(PastePayload::text("三".to_owned())),
    ] {
        assert!(app.handle(input, &mut ids, &clock).is_empty());
    }
    assert_eq!(
        app.editor_snapshot().expect("blocked editor").content,
        "active"
    );

    let effects = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    let sequences = effects
        .iter()
        .map(|effect| match effect {
            Effect::CommitRevision(revision) => revision.sequence,
            other => panic!("unexpected replay effect: {other:?}"),
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(sequences.len(), 3);
    assert!(
        sequences
            .iter()
            .all(|sequence| *sequence > capture.operation.sequence)
    );
    assert_eq!(
        app.editor_snapshot().expect("replayed editor").content,
        "active1two三"
    );
    assert_eq!(app.state.board.live_thoughts().len(), 2);
}

#[test]
fn commit_barrier_replays_pointer_and_preserves_resize_and_focus_signals() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
    let original = layout.thoughts[0].text_area;
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(32)]);
    let capture = next_commit(&mut app, &mut ids, &clock);

    assert!(
        app.handle(
            UiInput::Pointer(PointerInput {
                column: original.x,
                row: original.y,
                kind: PointerKind::Down(PointerButton::Left),
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        )
        .is_empty()
    );
    assert!(
        app.handle(
            UiInput::Resize {
                width: 30,
                height: 6,
            },
            &mut ids,
            &clock,
        )
        .is_empty()
    );
    assert!(app.layout.is_none());
    assert_eq!(
        app.handle(UiInput::HostFocusGained, &mut ids, &clock),
        vec![Effect::DiscoverAgents]
    );

    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert_ne!(app.state.focused_thought, capture_thought_id(&capture));
    assert_eq!(
        app.state.mode,
        InteractionMode::Edit {
            thought_id: original_id
        }
    );
}

#[test]
fn deferred_pointer_clicks_use_receipt_time_for_single_double_and_triple_clicks() {
    let (mut app, mut ids, mut receipt_clock, thought_id) = app_with_thought();
    app.state.mode = InteractionMode::Edit { thought_id };
    app.sync_editor_from_state();
    app.screenshot_started(std::time::Duration::ZERO);
    let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
    let text = layout.thoughts[0].text_area;
    let click = UiInput::Pointer(PointerInput {
        column: text.x + 2,
        row: text.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    });
    let UiInput::Pointer(pointer) = click.clone() else {
        panic!("pointer input");
    };
    assert!(matches!(
        app.hit(pointer),
        Some(crate::ui::HitTarget::Thought(_))
    ));
    app.queue_screenshot_candidates([candidate(66)]);
    let capture = next_commit(&mut app, &mut ids, &receipt_clock);
    app.handle(click.clone(), &mut ids, &receipt_clock);
    receipt_clock.set(Timestamp::from_millis(603));
    app.handle(click.clone(), &mut ids, &receipt_clock);
    assert_eq!(
        app.screenshot
            .deferred_inputs
            .iter()
            .map(|deferred| deferred.received_at)
            .collect::<Vec<_>>(),
        vec![Timestamp::from_millis(2), Timestamp::from_millis(603)]
    );
    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &receipt_clock);
    assert_eq!(app.pointer_click_count(), Some(1));

    let (mut app, mut ids, mut receipt_clock, thought_id) = app_with_thought();
    app.state.mode = InteractionMode::Edit { thought_id };
    app.sync_editor_from_state();
    app.screenshot_started(std::time::Duration::ZERO);
    let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
    let click = UiInput::Pointer(PointerInput {
        column: layout.thoughts[0].text_area.x + 2,
        row: layout.thoughts[0].text_area.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    });
    let UiInput::Pointer(pointer) = click.clone() else {
        panic!("pointer input");
    };
    assert!(matches!(
        app.hit(pointer),
        Some(crate::ui::HitTarget::Thought(_))
    ));
    app.queue_screenshot_candidates([candidate(67)]);
    let capture = next_commit(&mut app, &mut ids, &receipt_clock);
    for at in [10, 200, 350] {
        receipt_clock.set(Timestamp::from_millis(at));
        app.handle(click.clone(), &mut ids, &receipt_clock);
    }
    assert_eq!(
        app.screenshot
            .deferred_inputs
            .iter()
            .map(|deferred| deferred.received_at)
            .collect::<Vec<_>>(),
        [10, 200, 350]
            .into_iter()
            .map(Timestamp::from_millis)
            .collect::<Vec<_>>()
    );
    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &receipt_clock);
    assert_eq!(app.pointer_click_count(), Some(3));
}

#[test]
fn passive_motion_and_resize_do_not_consume_deliberate_replay_capacity() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(33)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    let movement = UiInput::Pointer(PointerInput {
        column: 0,
        row: 0,
        kind: PointerKind::Move,
        extend_selection: false,
    });
    for index in 0..300 {
        app.handle(movement.clone(), &mut ids, &clock);
        app.handle(
            UiInput::Resize {
                width: 30 + (index % 2),
                height: 6,
            },
            &mut ids,
            &clock,
        );
    }
    let deliberate = [
        UiInput::Key(UiKey::Character('k')),
        UiInput::PasteAnnotated(PastePayload::text("annotated".to_owned())),
        UiInput::Paste("plain".to_owned()),
        UiInput::Pointer(PointerInput {
            column: 1,
            row: 1,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        UiInput::Pointer(PointerInput {
            column: 2,
            row: 1,
            kind: PointerKind::Drag(PointerButton::Left),
            extend_selection: false,
        }),
        UiInput::Pointer(PointerInput {
            column: 2,
            row: 1,
            kind: PointerKind::ScrollDown,
            extend_selection: false,
        }),
    ];
    for input in deliberate.clone() {
        app.handle(input, &mut ids, &clock);
    }
    assert_eq!(app.deferred_deliberate_count(), deliberate.len());
    let queued = app
        .screenshot
        .deferred_inputs
        .iter()
        .filter(|deferred| deferred.input.is_deliberate_interaction())
        .map(|deferred| deferred.input.clone())
        .collect::<Vec<_>>();
    assert_eq!(queued, deliberate);
    let effects = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert!(app.screenshot.deferred_inputs.is_empty());
    assert_eq!(
        effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::CommitBoardOperation(_) | Effect::CommitRevision(_)
                )
            })
            .count(),
        2
    );
}

#[test]
fn deliberate_capacity_backpressures_the_runner_while_passive_input_proceeds() {
    let (mut app, mut ids, clock, thought_id) = app_with_thought();
    app.state.mode = InteractionMode::Edit { thought_id };
    app.sync_editor_from_state();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(68)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    for _ in 0..64 {
        assert!(app.screenshot_barrier_accepts(&UiInput::Key(UiKey::Character('x'))));
        app.handle(UiInput::Key(UiKey::Character('x')), &mut ids, &clock);
    }
    assert!(!app.screenshot_barrier_accepts(&UiInput::Paste("held".to_owned())));
    assert!(app.screenshot_barrier_accepts(&UiInput::Resize {
        width: 41,
        height: 9,
    }));
    assert!(
        app.screenshot_barrier_accepts(&UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Move,
            extend_selection: false,
        }))
    );

    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert_eq!(
        app.editor_snapshot().expect("replayed editor").content,
        format!("active{}", "x".repeat(64))
    );
}

#[test]
fn board_key_and_board_pastes_replay_without_capture_mode_stealing() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(34)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Duplicate), &mut ids, &clock);
    let effects = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_ne!(app.state.focused_thought, capture_thought_id(&capture));
    assert_eq!(app.state.board.live_thoughts().len(), 3);

    for (byte, input, expected) in [
        (35, UiInput::Paste("plain paste".to_owned()), "plain paste"),
        (
            36,
            UiInput::PasteAnnotated(PastePayload::text("annotated paste".to_owned())),
            "annotated paste",
        ),
    ] {
        let (mut app, mut ids, clock, _) = app_with_thought();
        app.screenshot_started(std::time::Duration::ZERO);
        app.queue_screenshot_candidates([candidate(byte)]);
        let capture = next_commit(&mut app, &mut ids, &clock);
        app.handle(input, &mut ids, &clock);
        let effects = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
        assert!(matches!(
            effects.as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        assert_ne!(app.state.focused_thought, capture_thought_id(&capture));
        assert!(matches!(app.state.mode, InteractionMode::Edit { .. }));
        assert_eq!(app.state.board.live_thoughts().len(), 3);
        assert!(
            app.state
                .board
                .live_thoughts()
                .iter()
                .any(|thought| thought.content == expected)
        );
    }
}

fn capture_thought_id(
    capture: &crate::ports::store::CaptureCommit,
) -> Option<crate::domain::ThoughtId> {
    match &capture.operation.forward {
        crate::domain::BoardMutation::AddThought { thought } => Some(thought.id),
        _ => None,
    }
}
