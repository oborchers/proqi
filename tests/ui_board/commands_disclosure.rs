//! Contextual Commands projection, disclosure, geometry, and activation parity.

use super::*;

#[path = "commands_disclosure/refresh.rs"]
mod refresh;

fn open(fixture: &mut Fixture) {
    fixture.input(crate::key_input(UiKey::Character(':')));
}

fn saved_thought(fixture: &mut Fixture, content: &str) {
    let sequence = fixture.paste(content);
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(crate::key_input(UiKey::Escape));
}

fn type_query(fixture: &mut Fixture, query: &str) {
    for character in query.chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
}

fn trim_snapshot_rows(rendered: &str) -> String {
    rendered
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn move_down(fixture: &mut Fixture, count: usize) {
    for _ in 0..count {
        fixture.input(crate::key_input(UiKey::Move {
            movement: proqi::ports::editor::CursorMovement::VisualDown,
            extend_selection: false,
        }));
    }
}

fn searched_row(fixture: &mut Fixture, label: &str, width: u16) -> (bool, String) {
    type_query(fixture, label);
    let (_, rows, _) = fixture.app.palette_view().expect("searched Commands");
    let index = rows
        .iter()
        .position(|row| row == label)
        .expect("exact command row");
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, 18));
    let enabled = layout
        .overlay
        .as_ref()
        .and_then(|overlay| overlay.item_interactive.get(index))
        .copied()
        .expect("command hit state");
    let rendered = text(draw(fixture, width, 18).backend().buffer());
    (enabled, rendered)
}

fn expand_by_keyboard(fixture: &mut Fixture) {
    let (_, rows, _) = fixture.app.palette_view().expect("Commands");
    let more = rows
        .iter()
        .position(|row| row == "More commands...")
        .expect("More commands control");
    move_down(fixture, more);
    fixture.input(crate::key_input(UiKey::Enter));
}

#[test]
fn concise_projection_is_stable_semantic_and_never_selects_a_destructive_action() {
    let mut empty = Fixture::new();
    empty.input(crate::key_input(UiKey::Escape));
    open(&mut empty);
    let (_, empty_rows, selected) = empty.app.palette_view().expect("empty Commands");
    assert_eq!(
        empty_rows,
        [
            "New thought",
            "Paste exactly",
            "Rename session",
            "Copy resume command",
            "Open contextual help",
            "Quit Proqi",
            "More commands...",
        ]
    );
    assert_eq!(selected, 0);

    let mut ordinary = Fixture::new();
    saved_thought(&mut ordinary, "ordinary saved thought");
    open(&mut ordinary);
    let (_, rows, selected) = ordinary.app.palette_view().expect("ordinary Commands");
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[selected], "New thought");
    assert!(
        !matches!(
            rows[selected].as_str(),
            "Delete thought"
                | "Cut thought"
                | "Submit"
                | "Send to another Proqi session and remove thought"
        ),
        "initial row must not be destructive"
    );
}

#[test]
fn more_commands_expands_in_place_by_keyboard_and_mouse() {
    let mut keyboard = Fixture::new();
    saved_thought(&mut keyboard, "keyboard");
    open(&mut keyboard);
    expand_by_keyboard(&mut keyboard);
    let (query, expanded, selected) = keyboard.app.palette_view().expect("expanded Commands");
    assert!(query.is_empty());
    assert_eq!(expanded.len(), 57);
    assert_eq!(selected, 0);

    let mut mouse = Fixture::new();
    saved_thought(&mut mouse, "mouse");
    open(&mut mouse);
    let (_, rows, _) = mouse.app.palette_view().expect("concise Commands");
    let more = rows
        .iter()
        .position(|row| row == "More commands...")
        .expect("More commands control");
    let layout = mouse.app.prepare_frame(Rect::new(0, 0, 72, 16));
    let area = layout.overlay.expect("Commands geometry").items[more];
    mouse.pointer(area.x, area.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        mouse.app.palette_view().expect("expanded Commands").1.len(),
        57
    );
}

#[test]
fn search_uses_complete_inventory_before_expansion_and_clear_restores_prior_view() {
    let mut concise = Fixture::new();
    saved_thought(&mut concise, "search source");
    open(&mut concise);
    assert!(
        !concise
            .app
            .palette_view()
            .expect("concise")
            .1
            .contains(&"Delete sentence".to_owned())
    );
    type_query(&mut concise, "delete sentence");
    assert_eq!(
        concise.app.palette_view().expect("searched").1,
        ["Delete sentence"]
    );
    for _ in 0.."delete sentence".chars().count() {
        concise.input(crate::key_input(UiKey::Backspace));
    }
    assert!(
        concise
            .app
            .palette_view()
            .expect("restored concise")
            .1
            .contains(&"More commands...".to_owned())
    );

    expand_by_keyboard(&mut concise);
    type_query(&mut concise, "updates");
    for _ in 0.."updates".chars().count() {
        concise.input(crate::key_input(UiKey::Backspace));
    }
    assert_eq!(
        concise
            .app
            .palette_view()
            .expect("restored expanded")
            .1
            .len(),
        57
    );
}

#[test]
fn disabled_rows_and_category_headings_are_not_pointer_targets() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    open(&mut fixture);
    type_query(&mut fixture, "delete thought");
    let before = fixture.app.state.clone();
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 12));
    let overlay = layout.overlay.as_ref().expect("searched Commands geometry");
    assert_eq!(overlay.item_interactive, [false]);
    let disabled = overlay.items[0];
    assert_eq!(layout.hit_test(disabled.x, disabled.y), None);
    fixture.pointer(
        disabled.x,
        disabled.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(crate::key_input(UiKey::Enter));
    assert_eq!(fixture.app.state, before);
    assert!(fixture.app.palette_view().is_some());
    let rendered = text(draw(&mut fixture, 72, 12).backend().buffer());
    assert!(rendered.contains("No thought is focused"));

    for _ in 0.."delete thought".chars().count() {
        fixture.input(crate::key_input(UiKey::Backspace));
    }
    expand_by_keyboard(&mut fixture);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 20));
    let overlay = layout.overlay.as_ref().expect("expanded Commands geometry");
    let heading = overlay
        .item_headings
        .iter()
        .flatten()
        .next()
        .copied()
        .expect("category heading");
    assert_eq!(layout.hit_test(heading.x, heading.y), None);
}

#[test]
fn zero_results_and_shallow_expanded_scrolling_remain_truthful() {
    let mut fixture = Fixture::new();
    saved_thought(&mut fixture, "scroll source");
    open(&mut fixture);
    type_query(&mut fixture, "no such command zzz");
    let (_, rows, selected) = fixture.app.palette_view().expect("zero results");
    assert_eq!(rows, ["No matching commands"]);
    assert_eq!(selected, 0);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 24, 5));
    assert_eq!(
        layout.overlay.expect("zero geometry").item_interactive,
        [false]
    );

    fixture.input(crate::key_input(UiKey::Escape));
    open(&mut fixture);
    expand_by_keyboard(&mut fixture);
    let _ = draw(&mut fixture, 24, 5);
    move_down(&mut fixture, 30);
    let _ = draw(&mut fixture, 24, 5);
    let (_, visible, selected) = fixture.app.palette_view().expect("scrolled expanded");
    assert!(selected < visible.len());
    assert!(!visible[selected].is_empty());
}

#[test]
fn concise_and_expanded_views_have_representative_responsive_snapshots() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.board]\n\"thought.insert_above\"=[{key='F5'}]\n\"thought.insert_below\"=[{key='F6'}]",
        )
        .expect("platform-neutral snapshot keymap"),
        ..UiSettings::default()
    };
    let mut concise = Fixture::with_settings(settings);
    saved_thought(&mut concise, "A calm contextual Commands source");
    open(&mut concise);
    insta::assert_snapshot!(
        "commands_concise_standard",
        trim_snapshot_rows(&text(draw(&mut concise, 72, 16).backend().buffer()))
    );

    expand_by_keyboard(&mut concise);
    insta::assert_snapshot!(
        "commands_expanded_narrow",
        text(draw(&mut concise, 34, 12).backend().buffer())
    );
}

#[test]
fn empty_insertion_and_editor_selection_contexts_report_exact_capabilities() {
    let mut empty = Fixture::new();
    empty.input(crate::key_input(UiKey::Escape));
    open(&mut empty);
    let (enabled, rendered) = searched_row(&mut empty, "Delete thought", 72);
    assert!(!enabled);
    assert!(rendered.contains("No thought is focused"));

    let mut insertion = Fixture::new();
    saved_thought(&mut insertion, "insertion source");
    insertion.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert!(insertion.app.insertion_focused());
    open(&mut insertion);
    let (enabled, _) = searched_row(&mut insertion, "Go to first thought", 72);
    assert!(enabled, "boundary focus remains available from insertion");

    let mut no_selection = Fixture::new();
    saved_thought(&mut no_selection, "editor source");
    no_selection.input(crate::key_input(UiKey::Character('e')));
    no_selection.input(crate::key_input(UiKey::Escape));
    open(&mut no_selection);
    let (enabled, rendered) =
        searched_row(&mut no_selection, "Extract selection as new thought", 78);
    assert!(!enabled);
    assert!(rendered.contains("Select text in the editor first"));

    let mut selection = Fixture::new();
    saved_thought(&mut selection, "selected editor source");
    selection.input(crate::key_input(UiKey::Character('e')));
    selection.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    selection.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    selection.input(crate::key_input(UiKey::Escape));
    open(&mut selection);
    let (enabled, _) = searched_row(&mut selection, "Extract selection as new thought", 78);
    assert!(enabled);
}

#[test]
fn selection_submission_and_history_change_relevance_without_changing_inventory() {
    let mut selection = Fixture::new();
    for thought in ["one", "two", "three"] {
        saved_thought(&mut selection, thought);
    }
    selection.input(crate::key_input(UiKey::UnmodifiedSpace));
    open(&mut selection);
    let (enabled, rendered) = searched_row(&mut selection, "Merge selected thoughts", 72);
    assert!(!enabled);
    assert!(rendered.contains("Select at least two thoughts"));

    selection.input(crate::key_input(UiKey::Escape));
    selection.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualUp,
        extend_selection: false,
    }));
    selection.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualUp,
        extend_selection: false,
    }));
    selection.input(crate::key_input(UiKey::UnmodifiedSpace));
    open(&mut selection);
    let (enabled, rendered) = searched_row(&mut selection, "Merge selected thoughts", 72);
    assert!(!enabled);
    assert!(rendered.contains("Selection must be contiguous"));

    let mut submission = Fixture::new();
    saved_thought(&mut submission, "submission source");
    open(&mut submission);
    let (enabled, rendered) = searched_row(&mut submission, "Submit and keep", 72);
    assert!(!enabled);
    assert!(rendered.contains("No verified agent is available"));
    submission.input(crate::key_input(UiKey::Escape));
    submission
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(
            proqi::domain::Direction::Right,
            "w1:p2",
        )]));
    open(&mut submission);
    assert!(searched_row(&mut submission, "Submit and keep", 72).0);
    submission.app.complete_agent_discovery(Ok(Vec::new()));
    let layout = submission.app.prepare_frame(Rect::new(0, 0, 72, 12));
    let (_, rows, _) = submission.app.palette_view().expect("Commands");
    let submit = rows
        .iter()
        .position(|row| row == "Submit and keep")
        .expect("exact submit row");
    assert!(!layout.overlay.expect("Commands").item_interactive[submit]);
    assert!(
        text(draw(&mut submission, 72, 12).backend().buffer())
            .contains("No verified agent is available")
    );
    assert!(
        submission
            .effects(crate::key_input(UiKey::Enter))
            .is_empty()
    );
    assert!(submission.app.palette_view().is_some());

    let mut history = Fixture::new();
    saved_thought(&mut history, "history source");
    open(&mut history);
    assert!(searched_row(&mut history, "Undo board action", 72).0);
    history.input(crate::key_input(UiKey::Enter));
    assert!(history.app.state.board.live_thoughts().is_empty());
}

#[test]
fn recovery_capacity_and_export_gate_retry_and_quit_truthfully() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("must export");
    fixture.app.acknowledge_persistence_result(
        sequence,
        Err(proqi::application::FailureCode::RecoveryCapacity),
    );

    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        ':',
    ))));
    let (retry_enabled, retry_rendered) = searched_row(&mut fixture, "Retry failed save", 76);
    assert!(!retry_enabled);
    assert!(retry_rendered.contains("Retry unavailable; export recovery instead"));

    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        ':',
    ))));
    let (quit_enabled, quit_rendered) = searched_row(&mut fixture, "Quit Proqi", 76);
    assert!(!quit_enabled);
    assert!(quit_rendered.contains("Export recovery before quitting"));

    fixture.input(crate::key_input(UiKey::Escape));
    let effects = fixture.effects(crate::key_input(UiKey::Character('w')));
    let [Effect::ExportRecovery { request_id, .. }] = effects.as_slice() else {
        panic!("expected recovery export");
    };
    let request_id = *request_id;
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        ':',
    ))));
    assert!(!searched_row(&mut fixture, "Quit Proqi", 76).0);
    fixture.app.complete_recovery_export(
        request_id,
        Ok(std::path::PathBuf::from(
            "/private/tmp/proqi-recovery-test.json",
        )),
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 76, 18));
    let (_, rows, _) = fixture.app.palette_view().expect("Commands");
    let quit = rows
        .iter()
        .position(|row| row == "Quit Proqi")
        .expect("Quit command");
    assert!(layout.overlay.expect("Commands").item_interactive[quit]);
    assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
    assert!(fixture.app.quit);
}

#[test]
fn keyboard_and_mouse_activation_share_typed_execution_and_disabled_movement_is_passive() {
    let mut keyboard = Fixture::new();
    saved_thought(&mut keyboard, "keyboard rename");
    open(&mut keyboard);
    type_query(&mut keyboard, "rename session");
    keyboard.input(crate::key_input(UiKey::Enter));
    assert_eq!(keyboard.app.session_rename_view(), Some(""));

    let mut mouse = Fixture::new();
    saved_thought(&mut mouse, "mouse rename");
    open(&mut mouse);
    type_query(&mut mouse, "rename session");
    let item = mouse
        .app
        .prepare_frame(Rect::new(0, 0, 72, 12))
        .overlay
        .expect("rename command geometry")
        .items[0];
    mouse.pointer(item.x, item.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(mouse.app.session_rename_view(), Some(""));

    let mut movement = Fixture::new();
    saved_thought(&mut movement, "only thought");
    open(&mut movement);
    let (enabled, rendered) = searched_row(&mut movement, "Move thought up", 72);
    assert!(!enabled);
    assert!(rendered.contains("Nothing to reorder"));
}
