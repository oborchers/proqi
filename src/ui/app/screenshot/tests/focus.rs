use super::behavior::{app_with_thought, candidate, created, next_commit};
use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{InstallationKind, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::CaptureOwnerInfo,
    },
    ui::BoardApp,
};

#[test]
fn every_overlay_keeps_capture_delivery_quiet() {
    interaction_stays("help", |app, _, _| app.help = true, |app| app.help);
    interaction_stays(
        "palette",
        |app, _, _| app.open_palette(),
        |app| app.palette.is_some(),
    );
    interaction_stays(
        "search",
        |app, _, _| app.open_search(),
        |app| app.search.is_some(),
    );
    interaction_stays(
        "rename",
        |app, _, _| app.begin_session_rename(),
        |app| app.rename.is_some(),
    );
    interaction_stays(
        "transfer",
        |app, ids, clock| {
            let _effects = app.begin_session_transfer(false, ids, clock);
        },
        |app| app.transfer.is_some(),
    );
    interaction_stays(
        "update",
        |app, _, _| {
            app.present_update(
                StableVersion::parse("1.2.3").expect("version"),
                InstallationKind::StandaloneArchive,
                2,
            );
        },
        |app| app.update_prompt.is_some(),
    );
    interaction_stays(
        "invocation",
        |app, _, _| {
            app.open_invocation_picker();
        },
        |app| app.invocation_popup.is_some(),
    );
    interaction_stays(
        "takeover",
        |app, ids, _| {
            app.screenshot_conflict(CaptureOwnerInfo {
                instance_id: ids.instance_id(),
                session_id: app.state.board.session.id,
                pid: 42,
                version: "test".to_owned(),
                capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
                control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
                control_endpoint: "private-control-endpoint".to_owned(),
                started_at: Timestamp::from_millis(1),
            });
        },
        |app| app.screenshot.takeover.is_some(),
    );
}

#[test]
fn live_board_range_survives_capture_without_focus_or_mode_change() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    let effects = app.reduce(Action::CreateThought {
        thought_id: ids.thought_id(),
        operation_id: ids.operation_id(),
        content: "second".to_owned(),
        annotations: Vec::new(),
        insertion_index: None,
        at: clock.now(),
    });
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("second thought operation");
    };
    app.acknowledge_persistence_result(operation.sequence, Ok(()));
    app.state.focused_thought = Some(original_id);
    app.state.mode = InteractionMode::Board;
    app.activate_range_latch();
    app.extend_range_by(1);
    assert_eq!(app.selection_len(), 2);
    let focused_before = app.state.focused_thought;

    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(45)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);

    assert_eq!(app.selection_len(), 2);
    assert_eq!(app.state.focused_thought, focused_before);
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_eq!(app.state.board.live_thoughts().len(), 3);
}

fn interaction_stays(
    label: &str,
    setup: impl FnOnce(
        &mut BoardApp,
        &mut crate::adapters::memory::FakeIdGenerator,
        &crate::adapters::memory::FakeClock,
    ),
    retained: impl FnOnce(&BoardApp) -> bool,
) {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(44)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    setup(&mut app, &mut ids, &clock);
    let mode = app.state.mode;
    app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert!(retained(&app), "{label} interaction was displaced");
    assert_eq!(app.state.focused_thought, Some(original_id), "{label}");
    assert_eq!(app.state.mode, mode, "{label}");
    assert_eq!(app.state.board.live_thoughts().len(), 2, "{label}");
}
