use super::*;

#[test]
fn icloud_and_downloading_use_exact_composable_labels_in_both_fold_states() {
    for (state, suffix) in [
        (AttachmentAvailability::InCloud, "in iCloud"),
        (AttachmentAvailability::Downloading, "downloading"),
    ] {
        for (image, noun) in [(true, "Image"), (false, "File")] {
            let mut fixture = Fixture::new();
            let path = "/private/var/folders/TemporaryItems/Grüße 第一.png";
            let effects = fixture.effects(UiInput::PasteAnnotated(attachment_payload(path, image)));
            let batch = attachment_batch(&effects);
            fixture
                .app
                .complete_attachment_checks(complete_availability(batch, state));

            let collapsed = text(draw(&mut fixture, 31, 8).backend().buffer());
            assert!(collapsed.contains(&format!("[{noun} 1 · {suffix}]")));
            assert!(!collapsed.contains("TemporaryItems"));

            let layout = fixture.app.prepare_frame(Rect::new(0, 0, 31, 8));
            let area = layout.thoughts[0].text_area;
            fixture.pointer(area.x + 2, area.y, PointerKind::Down(PointerButton::Left));
            fixture.input(crate::key_input(UiKey::Enter));
            let expanded = text(draw(&mut fixture, 80, 8).backend().buffer());
            assert!(expanded.contains(&format!("[{suffix}]")));
            assert_eq!(
                fixture.app.state.board.live_thoughts()[0].content,
                path,
                "presentation must preserve the exact canonical path"
            );
        }
    }
}

#[test]
fn cloud_states_refuse_preflight_until_the_same_attachment_is_available() {
    for state in [
        AttachmentAvailability::InCloud,
        AttachmentAvailability::Downloading,
    ] {
        let mut fixture = submission_fixture();
        let first = execute_palette(&mut fixture, "submit all and keep");
        let first = attachment_batch(&first);
        let refused = fixture
            .app
            .complete_attachment_checks(complete_availability(first, state));
        assert!(refused.is_empty(), "cloud state must not create a journal");
        assert_eq!(
            fixture.app.status_text(),
            Some("Proqi cannot access 1 attachment")
        );
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
        assert!(
            fixture
                .app
                .state
                .board
                .live_thoughts()
                .iter()
                .all(|thought| !fixture.app.state.thought_locked(thought.id))
        );

        let retry = execute_palette(&mut fixture, "submit all and keep");
        let retry = attachment_batch(&retry);
        let prepared = fixture
            .app
            .complete_attachment_checks(complete_availability(
                retry,
                AttachmentAvailability::Available,
            ));
        assert!(matches!(
            prepared.as_slice(),
            [Effect::PrepareSubmission(_)]
        ));
    }
}
