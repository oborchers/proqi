use super::*;
use proqi::ports::attachment_accessibility::{
    AttachmentAvailability, AttachmentCheckBatch, AttachmentCheckBatchResult, AttachmentCheckResult,
};

#[test]
fn asynchronous_submission_refresh_preserves_the_selected_typed_command() {
    let mut gained = Fixture::new();
    saved_thought(&mut gained, "discovery gain");
    open(&mut gained);
    let edit = gained
        .app
        .palette_view()
        .expect("Commands before discovery")
        .1
        .iter()
        .position(|row| row == "Edit thought")
        .expect("Edit command");
    move_down(&mut gained, edit);
    gained
        .app
        .complete_agent_discovery(Ok(vec![crate::agent::target(
            proqi::domain::Direction::Right,
            "w1:p2",
        )]));
    let (_, rows, selected) = gained.app.palette_view().expect("Commands after discovery");
    assert_eq!(rows[selected], "Edit thought");
    gained.input(crate::key_input(UiKey::Enter));
    assert!(gained.app.editor_snapshot().is_some());

    let mut lost = Fixture::new();
    saved_thought(&mut lost, "discovery loss");
    lost.app
        .complete_agent_discovery(Ok(vec![crate::agent::target(
            proqi::domain::Direction::Right,
            "w1:p2",
        )]));
    open(&mut lost);
    let rename = lost
        .app
        .palette_view()
        .expect("Commands before target loss")
        .1
        .iter()
        .position(|row| row == "Rename thought")
        .expect("Rename command");
    move_down(&mut lost, rename);
    lost.app.complete_agent_discovery(Ok(Vec::new()));
    let (_, rows, selected) = lost.app.palette_view().expect("Commands after target loss");
    assert_eq!(rows[selected], "Rename thought");
}

#[test]
fn commands_show_bindings_and_scope_from_the_captured_invocation_mode() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.board]\n\"clipboard.copy\"=[{key='F5'}]\n\"clipboard.paste_exact\"=[{key='F6'}]\n\"selection.select_all\"=[{key='F10'}]\n[bindings.edit]\n\"clipboard.copy\"=[{key='F7'}]\n\"clipboard.paste_exact\"=[{key='F8'}]\n\"commands.open\"=[{key='F9'}]\n\"selection.select_all\"=[{key='F11'}]",
        )
        .expect("contextual Commands bindings"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    saved_thought(&mut fixture, "contextual shortcuts");

    open(&mut fixture);
    let (_, rendered) = searched_row(&mut fixture, "Copy thought text", 88);
    assert!(rendered.contains("F5 · board"));
    fixture.input(crate::key_input(UiKey::Escape));
    open(&mut fixture);
    let (_, rendered) = searched_row(&mut fixture, "Paste exactly", 88);
    assert!(rendered.contains("F6 · board"));
    fixture.input(crate::key_input(UiKey::Escape));

    fixture.input(crate::key_input(UiKey::Character('e')));
    assert!(fixture.app.editor_snapshot().is_some());
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Copy thought text", 88);
    assert!(rendered.contains("F7 · edit"));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Paste exactly", 88);
    assert!(rendered.contains("F8 · edit"));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Select all items", 88);
    assert!(rendered.contains("F10 · selection"), "{rendered}");
    assert!(!rendered.contains("F11"));
}

#[test]
fn completed_manual_attachment_refresh_updates_open_commands_in_place() {
    let mut fixture = Fixture::new();
    let path = "/private/tmp/proqi-commands-refresh.txt";
    let effects = fixture.effects(UiInput::PasteAnnotated(
        PastePayload::annotated(
            path.to_owned(),
            vec![ContentAnnotation {
                start: 0,
                end: path.len(),
                kind: ContentAnnotationKind::Attachment {
                    ordinal: Some(1_u64.try_into().expect("attachment ordinal")),
                    image: false,
                    display_name: "proqi-commands-refresh.txt".to_owned(),
                },
            }],
        )
        .expect("valid attachment payload"),
    ));
    let initial = attachment_batch(&effects);
    fixture
        .app
        .complete_attachment_checks(complete_available(initial));
    fixture.input(crate::key_input(UiKey::Escape));

    let refresh = fixture.app.refresh_attachments(true);
    let refresh = attachment_batch(&refresh);
    open(&mut fixture);
    let (enabled, rendered) = searched_row(&mut fixture, "Refresh attachments", 82);
    assert!(!enabled);
    assert!(rendered.contains("Attachment refresh is in progress"));

    fixture
        .app
        .complete_attachment_checks(complete_available(refresh));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 82, 12));
    assert_eq!(
        layout.overlay.expect("refreshed Commands").item_interactive,
        [true]
    );
}

fn attachment_batch(effects: &[Effect]) -> AttachmentCheckBatch {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::CheckAttachments(batch) => Some(batch.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("attachment batch missing: {effects:?}"))
}

fn complete_available(batch: AttachmentCheckBatch) -> AttachmentCheckBatchResult {
    AttachmentCheckBatchResult {
        id: batch.id,
        purpose: batch.purpose,
        results: batch
            .checks
            .into_iter()
            .map(|key| AttachmentCheckResult {
                key,
                result: Ok(AttachmentAvailability::Available),
            })
            .collect(),
    }
}

#[test]
fn editor_clipboard_and_board_selection_commands_are_contextually_applicable() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"commands.open\"=[{key='F9'}]",
        )
        .expect("editor Commands binding"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    saved_thought(&mut fixture, "selection required");
    fixture.input(crate::key_input(UiKey::Character('e')));

    for command in [
        "Copy thought text",
        "Cut thought text",
        "Toggle item selection",
        "Start contiguous range selection",
    ] {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
            9,
        ))));
        let (enabled, rendered) = searched_row(&mut fixture, command, 88);
        assert!(!enabled, "{command} must not execute without its context");
        let reason = if command.starts_with("Copy") || command.starts_with("Cut") {
            "Select text in the editor first"
        } else {
            "Available from Board focus"
        };
        assert!(rendered.contains(reason));
        fixture.input(crate::key_input(UiKey::Escape));
    }

    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    for command in ["Copy thought text", "Cut thought text"] {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
            9,
        ))));
        assert!(searched_row(&mut fixture, command, 88).0);
        fixture.input(crate::key_input(UiKey::Escape));
    }
}
