use super::*;

#[test]
fn partial_attachment_transform_dissolves_annotation_and_preflight() {
    let path = "/tmp/Grüße 第一.png";
    let mut partial = Fixture::new();
    let effects = partial.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    let background = attachment_batch(&effects);
    partial
        .app
        .complete_attachment_checks(complete(background, Ok(())));
    partial.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    partial.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    partial.input(UiInput::Key(UiKey::Enter));
    partial.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..6 {
        partial.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    let transformed = partial.effects(UiInput::Key(UiKey::PrimaryCharacter('t')));
    assert!(
        transformed
            .iter()
            .all(|effect| !matches!(effect, Effect::CheckAttachments(_)))
    );
    let live = partial.app.state.board.live_thoughts();
    assert_eq!(
        live.iter()
            .map(|thought| thought.content.as_str())
            .collect::<String>(),
        path
    );
    assert!(live.iter().all(|thought| thought.annotations.is_empty()));
    partial.input(UiInput::Key(UiKey::Escape));
    partial.acknowledge_all_persistence();
    partial
        .app
        .complete_agent_discovery(Ok(vec![super::super::agent::target(
            Direction::Left,
            "w1:p2",
        )]));
    let submission = execute_palette(&mut partial, "submit all and keep");
    assert!(
        submission
            .iter()
            .all(|effect| !matches!(effect, Effect::CheckAttachments(_)))
    );
    assert!(
        submission
            .iter()
            .any(|effect| matches!(effect, Effect::PrepareSubmission(_)))
    );
}

#[test]
fn intact_attachment_transform_preserves_annotation_and_preflight() {
    let path = "/tmp/Grüße 第一.png";
    let mut intact = Fixture::new();
    let effects = intact.effects(UiInput::PasteAnnotated(attachment_payload(path, true)));
    intact
        .app
        .complete_attachment_checks(complete(attachment_batch(&effects), Ok(())));
    intact.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    let transformed = intact.effects(UiInput::Key(UiKey::PrimaryCharacter('t')));
    let moved_check = attachment_batch(&transformed);
    intact
        .app
        .complete_attachment_checks(complete(moved_check, Ok(())));
    let live = intact.app.state.board.live_thoughts();
    assert!(live[0].annotations.is_empty());
    assert_eq!(live[1].annotations.len(), 1);
    assert_eq!(live[1].content, path);
    intact.input(UiInput::Key(UiKey::Escape));
    intact.acknowledge_all_persistence();
    intact
        .app
        .complete_agent_discovery(Ok(vec![super::super::agent::target(
            Direction::Left,
            "w1:p2",
        )]));
    let submission = execute_palette(&mut intact, "submit all and keep");
    assert_eq!(attachment_batch(&submission).checks.len(), 1);
}
