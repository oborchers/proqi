//! Source consistency: the file holds save-time copy text and the Board keeps changed sources.

use super::{Fixture, written};
use crate::{
    domain::{BoardOperationKind, ExportDisposition, Timestamp},
    ports::environment::IdGenerator as _,
    ui::UiKey,
};

#[test]
fn sources_changed_during_the_write_are_kept_and_reported() {
    let mut fixture = Fixture::new(&[("body", None)]);
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Remove);
    let (request_id, request) = fixture.submit();
    let revision_id = fixture.ids.revision_id();
    crate::application::reduce(
        &mut fixture.app.state,
        crate::application::Action::EditThought {
            thought_id,
            revision_id,
            before_content: "body".to_owned(),
            after_content: "body changed".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: crate::domain::TextPosition::new(0, 0),
            after_cursor: crate::domain::TextPosition::new(0, 0),
            at: Timestamp::from_millis(2),
        },
    )
    .expect("external edit");
    assert!(
        fixture
            .complete(request_id, Ok(written(&request.path)))
            .is_empty()
    );
    assert_eq!(fixture.contents(), ["body changed"]);
    let status = fixture.app.status_text().expect("status");
    assert!(
        status.starts_with("file saved, but the board was kept: ") && status.len() > 36,
        "the reducer's own cause is kept: {status}"
    );
}

#[test]
fn the_file_holds_the_copy_text_at_save_time_and_removed_sources_abort() {
    let mut fixture = Fixture::new(&[("opened text", None)]);
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Keep);
    let revision_id = fixture.ids.revision_id();
    crate::application::reduce(
        &mut fixture.app.state,
        crate::application::Action::EditThought {
            thought_id,
            revision_id,
            before_content: "opened text".to_owned(),
            after_content: "edited by another owner".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: crate::domain::TextPosition::new(0, 0),
            after_cursor: crate::domain::TextPosition::new(0, 0),
            at: Timestamp::from_millis(2),
        },
    )
    .expect("external edit while the field is open");
    let (_, request) = fixture.submit();
    assert_eq!(request.content, "edited by another owner");

    let mut fixture = Fixture::new(&[("soon gone", None)]);
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Remove);
    let operation_id = fixture.ids.operation_id();
    crate::application::reduce(
        &mut fixture.app.state,
        crate::application::Action::DeleteThought {
            operation_id,
            thought_id,
            kind: BoardOperationKind::Delete,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("external delete while the field is open");
    assert!(fixture.key(UiKey::Enter).is_empty(), "nothing is written");
    assert!(fixture.app.export.active.is_none());
}
