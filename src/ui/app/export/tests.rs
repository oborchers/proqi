use std::path::{Path, PathBuf};

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect},
    domain::{
        BoardOperationKind, ExportDisposition, RequestId, Session, SessionBoard, Thought,
        ThoughtName, ThoughtPosition, Timestamp,
    },
    ports::{
        environment::IdGenerator as _,
        export::{
            DirectoryEntry, DirectoryListing, ExistingFile, ExportOverwrite, ExportWriteError,
            ExportWriteRequest, ExportWritten,
        },
    },
    ui::{BoardApp, UiKey, app::input_dispatch::ActiveInputOwner, input::RoutedInput as UiInput},
};

const SESSION_DIRECTORY: &str = "/work/project";

struct Fixture {
    app: BoardApp,
    ids: FakeIdGenerator,
    clock: FakeClock,
}

impl Fixture {
    fn new(bodies: &[(&str, Option<&str>)]) -> Self {
        let mut ids = FakeIdGenerator::new(1_790_342_412_000);
        let session = Session::new(
            ids.session_id(),
            PathBuf::from(SESSION_DIRECTORY),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let thoughts = bodies
            .iter()
            .enumerate()
            .map(|(index, (body, name))| {
                let mut thought = Thought::new(
                    ids.thought_id(),
                    session.id,
                    (*body).to_owned(),
                    ThoughtPosition::new(u32::try_from(index).expect("position")),
                    Timestamp::from_millis(1),
                );
                thought.set_name(name.map(|name| ThoughtName::new(name).expect("name")));
                thought
            })
            .collect();
        let board = SessionBoard::new(session, thoughts).expect("board");
        let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
        app.set_home_directory(Some(PathBuf::from("/home/tester")));
        Self {
            app,
            ids,
            clock: FakeClock::new(Timestamp::from_millis(1_790_342_412_345)),
        }
    }

    fn select_all(&mut self) {
        let ids = self.app.live_item_ids();
        for id in ids {
            self.app.state.focused_item = Some(id);
            self.app.toggle_selection();
        }
    }

    fn begin(&mut self, disposition: ExportDisposition) {
        let effects = self
            .app
            .begin_export(disposition, &mut self.ids, &self.clock);
        assert!(effects.is_empty(), "{effects:?}");
    }

    fn key(&mut self, key: UiKey) -> Vec<Effect> {
        self.app
            .handle_export_input(&UiInput::Key(key), &mut self.ids, &self.clock)
    }

    fn field(&self) -> String {
        match self.app.export_view() {
            Some(super::ExportView::Path { query, .. }) => query,
            _ => panic!("path stage"),
        }
    }

    fn set_field(&mut self, text: &str) {
        self.key(UiKey::SelectAll);
        self.app
            .handle_export_input(&UiInput::Paste(text.to_owned()), &mut self.ids, &self.clock);
    }

    fn submit(&mut self) -> (RequestId, ExportWriteRequest) {
        let effects = self.key(UiKey::Enter);
        let [
            Effect::WriteExport {
                request_id,
                request,
            },
        ] = effects.as_slice()
        else {
            panic!("one write: {effects:?}");
        };
        (*request_id, request.clone())
    }

    fn complete(
        &mut self,
        request_id: RequestId,
        result: Result<ExportWritten, ExportWriteError>,
    ) -> Vec<Effect> {
        self.app
            .complete_export_write(request_id, result, &mut self.ids, &self.clock)
    }

    fn contents(&self) -> Vec<String> {
        self.app
            .state
            .board
            .live_thoughts()
            .into_iter()
            .map(|thought| thought.content.clone())
            .collect()
    }
}

fn written(path: &Path) -> ExportWritten {
    ExportWritten {
        path: path.to_path_buf(),
        bytes: 1,
        replaced: false,
        unchanged: false,
    }
}

const EXISTING: ExistingFile = ExistingFile {
    device: 1,
    inode: 2,
    length: 3,
    modified_nanos: 4,
};

#[test]
fn default_destination_uses_the_session_directory_and_default_name() {
    let mut fixture = Fixture::new(&[("body", Some("Release plan"))]);
    fixture.begin(ExportDisposition::Keep);
    assert_eq!(fixture.field(), "/work/project/Release plan.txt");
    assert_eq!(
        fixture.app.export_input_owner(),
        Some(ActiveInputOwner::ExportPath)
    );
    fixture.key(UiKey::Escape);
    assert!(fixture.app.export.active.is_none());

    let mut fixture = Fixture::new(&[("one", Some("Named")), ("two", None)]);
    fixture.select_all();
    fixture.begin(ExportDisposition::Keep);
    let field = fixture.field();
    assert!(
        field.starts_with("/work/project/project-2026-09-25-"),
        "{field}"
    );
    assert_eq!(
        Path::new(&field)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("txt")
    );
}

#[test]
fn keep_writes_exact_copy_text_and_leaves_the_board_unchanged() {
    let mut fixture = Fixture::new(&[("first\r\n", None), ("  second ü", None)]);
    fixture.select_all();
    fixture.begin(ExportDisposition::Keep);
    fixture.set_field("~/notes.md");
    let (request_id, request) = fixture.submit();
    assert_eq!(request.path, PathBuf::from("/home/tester/notes.md"));
    assert_eq!(request.content, "first\r\n\n\n  second ü");
    assert_eq!(request.overwrite, ExportOverwrite::Refuse);
    assert!(
        fixture.key(UiKey::Escape).is_empty(),
        "writing ignores cancel"
    );
    assert!(fixture.app.export.active.is_some());
    assert!(
        fixture
            .complete(request_id, Ok(written(&request.path)))
            .is_empty()
    );
    assert!(fixture.app.export.active.is_none());
    assert_eq!(fixture.contents(), ["first\r\n", "  second ü"]);
}

#[test]
fn remove_and_replace_change_the_board_only_after_durable_success() {
    let mut fixture = Fixture::new(&[("first", None), ("second", None), ("kept", None)]);
    let ids = fixture.app.live_item_ids();
    for id in &ids[..2] {
        fixture.app.state.focused_item = Some(*id);
        fixture.app.toggle_selection();
    }
    fixture.begin(ExportDisposition::Remove);
    fixture.set_field("out.txt");
    let (request_id, request) = fixture.submit();
    assert_eq!(request.path, PathBuf::from("/work/project/out.txt"));
    assert_eq!(fixture.app.pending_mutation_intents().total(), 1);
    assert_eq!(
        fixture.contents().len(),
        3,
        "nothing changes before durability"
    );
    let effects = fixture.complete(request_id, Ok(written(&request.path)));
    let Some(Effect::CommitBoardOperation(operation)) = effects.first() else {
        panic!("board operation: {effects:?}");
    };
    assert_eq!(operation.kind, BoardOperationKind::ExportAndRemove);
    assert_eq!(fixture.contents(), ["kept"]);
    assert!(fixture.app.pending_mutation_intents().is_empty());

    let mut fixture = Fixture::new(&[("first", None), ("second", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::ReplaceWithReference);
    fixture.set_field("/tmp/ref file.txt");
    let (request_id, request) = fixture.submit();
    let effects = fixture.complete(request_id, Ok(written(&request.path)));
    let Some(Effect::CommitBoardOperation(operation)) = effects.first() else {
        panic!("board operation: {effects:?}");
    };
    assert_eq!(operation.kind, BoardOperationKind::ExportAndReplace);
    assert_eq!(fixture.contents(), ["/tmp/ref file.txt ", "second"]);
    let focused = fixture.app.state.focused_thought_id().expect("focus");
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(focused)
            .expect("reference")
            .content,
        "/tmp/ref file.txt "
    );
}

#[test]
fn an_existing_file_needs_confirmation_and_cancel_is_the_default() {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Remove);
    fixture.set_field("taken.txt");
    let (request_id, _) = fixture.submit();
    assert!(
        fixture
            .complete(request_id, Err(ExportWriteError::Exists(EXISTING)))
            .is_empty()
    );
    assert_eq!(
        fixture.app.export_input_owner(),
        Some(ActiveInputOwner::ExportReplace)
    );
    assert!(
        fixture.key(UiKey::Enter).is_empty(),
        "Cancel is the default"
    );
    assert_eq!(fixture.field(), "taken.txt");

    let (request_id, _) = fixture.submit();
    fixture.complete(request_id, Err(ExportWriteError::Exists(EXISTING)));
    fixture
        .app
        .move_export_choice(crate::ui::ListNavigation::Next);
    let effects = fixture.key(UiKey::Enter);
    let [
        Effect::WriteExport {
            request_id,
            request,
        },
    ] = effects.as_slice()
    else {
        panic!("confirmed write: {effects:?}");
    };
    assert_eq!(request.path, PathBuf::from("/work/project/taken.txt"));
    assert_eq!(request.overwrite, ExportOverwrite::Confirmed(EXISTING));
    let request_id = *request_id;
    assert!(
        fixture
            .complete(request_id, Err(ExportWriteError::Changed))
            .is_empty()
    );
    assert_eq!(
        fixture.contents(),
        ["body"],
        "a changed file keeps the board"
    );
    assert_eq!(
        fixture.app.export_input_owner(),
        Some(ActiveInputOwner::ExportPath)
    );
}

#[test]
fn write_failures_and_invalid_paths_never_change_the_board() {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::ReplaceWithReference);
    for invalid in ["", "folder/", "~"] {
        fixture.set_field(invalid);
        assert!(fixture.key(UiKey::Enter).is_empty(), "{invalid}");
    }
    for failure in [
        ExportWriteError::StorageFull,
        ExportWriteError::DirectoryMissing,
        ExportWriteError::PermissionDenied,
        ExportWriteError::TargetIsSymlink,
    ] {
        fixture.set_field("out.txt");
        let (request_id, _) = fixture.submit();
        assert!(fixture.complete(request_id, Err(failure)).is_empty());
        assert_eq!(fixture.contents(), ["body"]);
        assert_eq!(fixture.field(), "out.txt");
    }
    let stale = fixture.ids.request_id();
    assert!(
        fixture
            .complete(stale, Ok(written(Path::new("/x"))))
            .is_empty()
    );
}

#[test]
fn tab_completion_lists_once_then_cycles_and_ignores_stale_listings() {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Keep);
    fixture.set_field("docs/re");
    let effects = fixture.key(UiKey::Tab);
    let [
        Effect::ListExportDirectory {
            generation,
            directory,
        },
    ] = effects.as_slice()
    else {
        panic!("listing: {effects:?}");
    };
    assert_eq!(directory, Path::new("/work/project/docs/"));
    let generation = *generation;
    let listing = DirectoryListing {
        entries: ["report-a.txt", "report-b.txt"]
            .iter()
            .map(|name| DirectoryEntry {
                name: (*name).to_owned(),
                directory: false,
            })
            .collect(),
        truncated: false,
    };
    fixture
        .app
        .complete_export_listing(generation.wrapping_sub(1), Ok(listing.clone()));
    assert_eq!(fixture.field(), "docs/re", "stale generation ignored");
    fixture
        .app
        .complete_export_listing(generation, Ok(listing.clone()));
    assert_eq!(fixture.field(), "docs/report-");
    assert!(fixture.key(UiKey::Tab).is_empty(), "cycles without listing");
    assert_eq!(fixture.field(), "docs/report-a.txt");
    assert!(fixture.key(UiKey::Tab).is_empty());
    assert_eq!(fixture.field(), "docs/report-b.txt");
    assert!(fixture.key(UiKey::BackTab).is_empty());
    assert_eq!(fixture.field(), "docs/report-a.txt");
    fixture.key(UiKey::Undo);
    assert_eq!(
        fixture.field(),
        "docs/report-b.txt",
        "each completion is one undo step"
    );

    fixture.set_field("docs/x");
    let effects = fixture.key(UiKey::Tab);
    let [Effect::ListExportDirectory { generation, .. }] = effects.as_slice() else {
        panic!("listing");
    };
    let generation = *generation;
    fixture.key(UiKey::Character('y'));
    fixture.app.complete_export_listing(generation, Ok(listing));
    assert_eq!(
        fixture.field(),
        "docs/xy",
        "edited text is never overwritten"
    );
}

#[test]
fn pointer_rows_save_and_complete() {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Keep);
    fixture.set_field("d");
    let effects = fixture.key(UiKey::Tab);
    let [Effect::ListExportDirectory { generation, .. }] = effects.as_slice() else {
        panic!("listing");
    };
    let generation = *generation;
    fixture.app.complete_export_listing(
        generation,
        Ok(DirectoryListing {
            entries: vec![
                DirectoryEntry {
                    name: "docs".to_owned(),
                    directory: true,
                },
                DirectoryEntry {
                    name: "drafts".to_owned(),
                    directory: true,
                },
            ],
            truncated: false,
        }),
    );
    assert!(
        fixture
            .app
            .activate_export_row(2, &mut fixture.ids)
            .is_empty()
    );
    assert_eq!(fixture.field(), "drafts/");
    fixture.set_field("drafts/out.txt");
    let effects = fixture.app.activate_export_row(0, &mut fixture.ids);
    assert!(matches!(effects.as_slice(), [Effect::WriteExport { .. }]));
}

#[path = "tests/rendering.rs"]
mod rendering;
#[path = "tests/sources.rs"]
mod sources;
