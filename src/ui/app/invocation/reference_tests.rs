use std::path::Path;

use ratatui_core::{backend::TestBackend, terminal::Terminal};

use crate::{
    application::Effect,
    ports::{
        agent::{AgentState, HarnessKind},
        editor::CursorMovement,
        invocation::{
            InvocationDiscovery, InvocationEntry, InvocationReferenceProvider, LiveAgentReference,
        },
    },
    ui::{
        PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey, render,
    },
};

use super::contract::{app, entry};

fn reference(
    name: &str,
    workspace: (&str, Option<&str>),
    tab: (&str, Option<&str>),
    pane: &str,
    state: AgentState,
) -> LiveAgentReference {
    LiveAgentReference::new(
        InvocationReferenceProvider::Herdr,
        name.to_owned(),
        HarnessKind::new("codex").expect("harness"),
        workspace.0.to_owned(),
        workspace.1.map(str::to_owned),
        tab.0.to_owned(),
        tab.1.map(str::to_owned),
        pane.to_owned(),
        state,
    )
    .expect("live reference")
}

fn reviewer(state: AgentState) -> LiveAgentReference {
    reference(
        "reviewer",
        ("w2", Some("Product")),
        ("w2:t4", Some("Review")),
        "w2:p9",
        state,
    )
}

fn install_live(
    app: &mut crate::ui::BoardApp,
    cwd: &Path,
    project: Vec<InvocationEntry>,
    live: Vec<LiveAgentReference>,
) -> u64 {
    let effects = app.refresh_invocations();
    let [Effect::DiscoverInvocations(request)] = effects.as_slice() else {
        panic!("refresh effect");
    };
    let generation = request.generation;
    app.complete_invocation_discovery(Ok(InvocationDiscovery {
        generation,
        cwd: cwd.to_owned(),
        global: Vec::new(),
        project,
        live,
    }));
    generation
}

#[test]
fn manual_picker_keeps_installed_entries_and_truthfully_groups_duplicate_agent_names() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        vec![entry(
            "$review",
            crate::ports::invocation::InvocationKind::Skill,
            crate::ports::invocation::InvocationScope::Project,
        )],
        vec![
            reviewer(AgentState::Working),
            reference(
                "builder",
                ("w2", Some("Product")),
                ("w2:t4", Some("Review")),
                "w2:p10",
                AgentState::Idle,
            ),
            reference(
                "reviewer",
                ("w3", None),
                ("w3:t2", None),
                "w3:p1",
                AgentState::Idle,
            ),
        ],
    );
    app.open_invocation_picker();

    let choices = app.invocation_view().expect("picker").1;
    assert_eq!(choices.len(), 4);
    assert_eq!(choices[0].token, "$review");
    assert_eq!(choices[1].token, "reviewer");
    assert_eq!(choices[1].qualifier, "codex w2:p9 working");
    assert_eq!(
        choices[1].group.as_deref(),
        Some("Live in Herdr · w2/w2:t4 · Product/Review")
    );
    assert_eq!(choices[2].group, None);
    assert_eq!(
        choices[3].group.as_deref(),
        Some("Live in Herdr · w3/w3:t2")
    );
}

#[test]
fn automatic_selection_inserts_an_inert_location_and_readiness_is_display_only() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("Ask @", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![reviewer(AgentState::Working)],
    );
    let effects = app.handle(UiInput::Key(UiKey::Character('r')), &mut ids, &clock);
    assert!(matches!(
        effects.as_slice(),
        [Effect::DiscoverInvocations(_)]
    ));

    let choice = &app.invocation_view().expect("automatic picker").1[0];
    assert!(choice.qualifier.contains("working"));
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let inserted = "Ask Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 ";
    assert_eq!(app.editor_snapshot().expect("editor").content, inserted);
    assert!(!inserted.contains("working"));

    app.handle(UiInput::Key(UiKey::Undo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "Ask @");
    app.handle(UiInput::Key(UiKey::Redo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, inserted);
}

#[test]
fn manual_selection_replaces_the_exact_unicode_selection_in_one_history_step() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let original = "α selected 🙂 omega";
    let (mut app, mut ids, clock) = app(original, cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![reviewer(AgentState::Idle)],
    );
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::DocumentStart,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::DocumentEnd,
            extend_selection: true,
        }),
        &mut ids,
        &clock,
    );
    app.open_invocation_picker();
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    let inserted = "Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 ";
    assert_eq!(app.editor_snapshot().expect("editor").content, inserted);
    app.handle(UiInput::Key(UiKey::Undo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, original);
    app.handle(UiInput::Key(UiKey::Redo), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, inserted);
}

#[test]
fn manual_live_reference_is_delimited_from_adjacent_prose() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("Coordinate", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![reviewer(AgentState::Idle)],
    );
    app.open_invocation_picker();
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "Coordinate Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}

#[test]
fn command_surface_refreshes_each_time_the_existing_picker_opens() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("prompt", cwd.path());
    app.open_palette();
    app.handle(
        UiInput::Paste("Insert invocation or Herdr reference".to_owned()),
        &mut ids,
        &clock,
    );
    let (_, matches, _) = app.palette_view().expect("palette");
    assert_eq!(matches, ["Insert invocation or Herdr reference"]);

    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(app.invocation_view().is_some());
    assert!(matches!(
        effects.as_slice(),
        [Effect::DiscoverInvocations(_)]
    ));
}

#[test]
fn newest_empty_result_wins_when_an_agent_disappears() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    let old_effects = app.refresh_invocations();
    let [Effect::DiscoverInvocations(old)] = old_effects.as_slice() else {
        panic!("old refresh");
    };
    let old_generation = old.generation;
    let newest_effects = app.refresh_invocations();
    let [Effect::DiscoverInvocations(newest)] = newest_effects.as_slice() else {
        panic!("new refresh");
    };
    let newest_generation = newest.generation;
    app.complete_invocation_discovery(Ok(InvocationDiscovery {
        generation: newest_generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: Vec::new(),
        live: Vec::new(),
    }));
    app.complete_invocation_discovery(Ok(InvocationDiscovery {
        generation: old_generation,
        cwd: cwd.path().to_owned(),
        global: Vec::new(),
        project: Vec::new(),
        live: vec![reviewer(AgentState::Working)],
    }));
    app.open_invocation_picker();
    assert!(app.invocation_view().expect("picker").1.is_empty());
}

#[test]
fn narrow_resized_mouse_geometry_selects_the_same_inert_reference() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![reviewer(AgentState::Idle)],
    );
    app.open_invocation_picker();
    let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 30, 6));
    let item = layout.overlay.expect("overlay").items[0];
    assert_eq!(item.height, 2);
    app.handle(
        UiInput::Pointer(PointerInput {
            column: item.x,
            row: item.y.saturating_add(1),
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}

#[test]
fn shallow_keyboard_scroll_keeps_the_active_workspace_and_tab_group_visible() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![
            reviewer(AgentState::Working),
            reference(
                "builder",
                ("w3", Some("Implementation")),
                ("w3:t2", Some("Build")),
                "w3:p7",
                AgentState::Idle,
            ),
        ],
    );
    app.open_invocation_picker();
    app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 30, 6));
    app.handle(UiInput::Key(UiKey::PickerNext), &mut ids, &clock);

    let (_, choices, selected) = app.invocation_view().expect("scrolled picker");
    assert_eq!(selected, 0);
    assert_eq!(choices[0].token, "builder");
    assert_eq!(
        choices[0].group.as_deref(),
        Some("Live in Herdr · w3/w3:t2 · Implementation/Build")
    );
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(
        app.editor_snapshot()
            .expect("editor")
            .content
            .starts_with("Herdr collaborator: builder (codex)")
    );
}

fn live_snapshot(width: u16, height: u16) -> String {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("Coordinate", cwd.path());
    install_live(
        &mut app,
        cwd.path(),
        Vec::new(),
        vec![
            reviewer(AgentState::Working),
            reference(
                "builder",
                ("w3", Some("Implementation")),
                ("w3:t2", Some("Build")),
                "w3:p7",
                AgentState::Idle,
            ),
        ],
    );
    app.open_invocation_picker();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn live_group_snapshot_covers_wide_layout() {
    insta::assert_snapshot!("invocation_live_wide", live_snapshot(72, 12));
}

#[test]
fn live_group_snapshot_covers_narrow_and_shallow_layout() {
    insta::assert_snapshot!("invocation_live_narrow_shallow", live_snapshot(30, 6));
}
