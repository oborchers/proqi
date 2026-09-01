use super::*;

#[test]
fn repeated_prose_edits_remap_one_manual_check_without_rechecking_the_board() {
    let (mut board, ids) = board_with_attachments(2);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(startup)).0.is_empty());

    let refresh = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    for suffix in [" first", " second"] {
        board
            .thought_mut(ids[0])
            .expect("thought")
            .content
            .push_str(suffix);
        assert!(
            state.reconcile(&board).is_empty(),
            "prose must not replace the in-flight filesystem batch"
        );
        assert!(!state.inaccessible(ids[0], 0));
    }

    let completion = completion(refresh, |key| {
        (key.thought_id != ids[0])
            .then_some(())
            .ok_or(AttachmentAccessFailure::Missing)
    });
    let (effects, _, outcome) = state.complete(completion);
    assert!(effects.is_empty());
    assert_eq!(outcome.expect("current exact completion").inaccessible, 1);
    assert!(state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[1], 0));
    assert!(!state.manual_refresh_active());
}

#[test]
fn relinking_a_later_batch_drops_the_obsolete_queued_path() {
    let (mut board, ids) = board_with_attachments(40);
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    let old_path = "/tmp/Grüße-39.png";
    let new_path = "/tmp/relinked-later.png";
    let thought = board.thought_mut(ids[39]).expect("later thought");
    thought.content = new_path.to_owned();
    thought.annotations[0].end = new_path.len();
    let ContentAnnotationKind::Attachment { display_name, .. } = &mut thought.annotations[0].kind
    else {
        panic!("attachment annotation");
    };
    *display_name = "relinked-later.png".to_owned();
    assert!(state.reconcile(&board).is_empty());

    let mut effects = state.complete(accessible(first)).0;
    let mut checked = Vec::new();
    while !effects.is_empty() {
        let batch = one_batch(effects);
        checked.extend(batch.checks.iter().map(|key| key.canonical_path.clone()));
        effects = state.complete(accessible(batch)).0;
    }
    assert_eq!(
        checked
            .iter()
            .filter(|path| path.as_str() == new_path)
            .count(),
        1
    );
    assert!(!checked.iter().any(|path| path == old_path));
}
