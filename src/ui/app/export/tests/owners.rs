//! Input ownership: the visible overlay owns keys, rows, and close; failures stay truthful.

use super::rendering::{click, confirm_stage};
use super::{Fixture, written};
use crate::{
    application::{DurabilityState, Effect, FailureCode},
    domain::{ExportDisposition, OperationSequence},
    ports::{environment::IdGenerator as _, runtime::CaptureOwnerInfo},
    ui::{HitTarget, UiInput, UiKey},
};

fn takeover_above_export(disposition: ExportDisposition) -> Fixture {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(disposition);
    fixture.set_field("out.txt");
    let session_id = fixture.app.state.board.session.id;
    let owner = CaptureOwnerInfo {
        instance_id: fixture.ids.instance_id(),
        session_id,
        pid: 42,
        version: "test".to_owned(),
        capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
        control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
        control_endpoint: "private-control-endpoint".to_owned(),
        started_at: crate::domain::Timestamp::from_millis(1),
    };
    fixture.app.screenshot_conflict(owner);
    fixture
}

#[test]
fn a_takeover_shown_above_export_owns_pointer_rows_and_close() {
    let mut fixture = takeover_above_export(ExportDisposition::Remove);
    assert!(
        fixture.app.screenshot_takeover_view().is_some(),
        "the takeover is on screen"
    );
    // Row 0 of the visible takeover is its Cancel, never the hidden save row.
    let effects = click(&mut fixture, HitTarget::PaletteItem(0));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::WriteExport { .. })),
        "{effects:?}"
    );
    assert!(fixture.app.screenshot_takeover_view().is_none());
    assert_eq!(fixture.field(), "out.txt", "export stays open beneath");
    assert_eq!(fixture.contents(), ["body"]);

    let mut fixture = takeover_above_export(ExportDisposition::ReplaceWithReference);
    assert!(click(&mut fixture, HitTarget::CloseOverlay).is_empty());
    assert!(
        fixture.app.screenshot_takeover_view().is_none(),
        "close dismisses the visible takeover"
    );
    assert_eq!(fixture.field(), "out.txt", "the export beneath stays open");
}

#[test]
fn a_takeover_shown_above_export_owns_keyboard_input() {
    let mut fixture = takeover_above_export(ExportDisposition::Remove);
    let effects = fixture
        .app
        .handle(UiInput::Key(UiKey::Enter), &mut fixture.ids, &fixture.clock);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::WriteExport { .. })),
        "{effects:?}"
    );
    assert!(fixture.app.screenshot_takeover_view().is_none());
    fixture.app.handle(
        UiInput::Key(UiKey::Escape),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(
        fixture.app.export.active.is_none(),
        "then Escape closes export"
    );
    assert_eq!(fixture.contents(), ["body"]);
}

#[test]
fn close_and_escape_both_leave_the_confirmation_for_the_path_field() {
    let mut fixture = confirm_stage();
    assert!(click(&mut fixture, HitTarget::CloseOverlay).is_empty());
    assert_eq!(
        fixture.field(),
        "notes/Grüße.txt",
        "pointer close keeps the path"
    );

    let mut fixture = confirm_stage();
    fixture.key(UiKey::Escape);
    assert_eq!(fixture.field(), "notes/Grüße.txt", "Escape keeps the path");
    assert_eq!(fixture.contents().len(), 2);
}

#[test]
fn board_changing_exports_do_not_open_while_saving_fails() {
    for disposition in [
        ExportDisposition::Remove,
        ExportDisposition::ReplaceWithReference,
    ] {
        let mut fixture = path_stage_with_failed_storage();
        let effects = fixture
            .app
            .begin_export(disposition, &mut fixture.ids, &fixture.clock);
        assert!(effects.is_empty());
        assert!(fixture.app.export.active.is_none(), "{disposition:?}");
        assert!(
            fixture
                .app
                .status_text()
                .is_some_and(|status| status.contains("cannot be removed or replaced"))
        );
    }
    let mut fixture = path_stage_with_failed_storage();
    fixture.begin(ExportDisposition::Keep);
    assert!(
        fixture.app.export.active.is_some(),
        "a plain export still works"
    );
}

fn path_stage_with_failed_storage() -> Fixture {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.app.state.durability = DurabilityState::Failed {
        durable: OperationSequence::ZERO,
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };
    fixture
}

#[test]
fn a_storage_failure_after_the_write_is_reported_as_the_cause() {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Remove);
    let (request_id, request) = fixture.submit();
    fixture.app.state.durability = DurabilityState::Failed {
        durable: OperationSequence::ZERO,
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };
    assert!(
        fixture
            .complete(request_id, Ok(written(&request.path)))
            .is_empty()
    );
    assert_eq!(fixture.contents(), ["body"]);
    let status = fixture.app.status_text().expect("status");
    assert!(
        status.starts_with("file saved, but the board was kept: "),
        "{status}"
    );
    assert!(!status.contains("thoughts changed"), "{status}");
}

#[test]
fn a_non_utf8_session_folder_prefills_nothing_and_says_why() {
    use std::os::unix::ffi::OsStrExt as _;
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.app.state.board.session.last_opened_cwd =
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(b"/work/caf\xe9"));
    fixture.begin(ExportDisposition::Keep);
    assert_eq!(fixture.field(), "");
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("not valid UTF-8"))
    );
}
