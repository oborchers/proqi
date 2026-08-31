use std::path::Path;

use crate::{
    application::Effect,
    domain::ContentAnnotationKind,
    ports::{
        agent::{AgentFailureCode, AgentState, HarnessKind},
        editor::CursorMovement,
        invocation::{
            InvocationDiscovery, InvocationEntry, InvocationReferenceDiscovery,
            InvocationReferenceProvider, LiveAgentReference,
        },
    },
    ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};

use super::contract::{app, entry};

fn reference(
    name: &str,
    workspace: (&str, Option<&str>),
    tab: (&str, Option<&str>),
    pane: &str,
    state: AgentState,
) -> LiveAgentReference {
    reference_kind(Some(name), "codex", workspace, tab, pane, state)
}

fn reference_kind(
    name: Option<&str>,
    harness: &str,
    workspace: (&str, Option<&str>),
    tab: (&str, Option<&str>),
    pane: &str,
    state: AgentState,
) -> LiveAgentReference {
    LiveAgentReference::new(
        InvocationReferenceProvider::Herdr,
        name.map(str::to_owned),
        HarnessKind::new(harness).expect("harness"),
        workspace.0.to_owned(),
        workspace.1.map(str::to_owned),
        tab.0.to_owned(),
        tab.1.map(str::to_owned),
        pane.to_owned(),
        state,
    )
    .expect("live reference")
}

#[test]
fn rows_deduplicate_names_and_use_real_topology_labels() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(
        &mut app,
        vec![
            reference_kind(
                None,
                "codex",
                ("w2", Some("Meta")),
                ("w2:t1", Some("codex")),
                "w2:p1",
                AgentState::Idle,
            ),
            reference_kind(
                Some("coaching-philipp"),
                "claude",
                ("w4", Some("Consulting")),
                ("w4:t2", Some("coaching-philipp")),
                "w4:p2",
                AgentState::Idle,
            ),
            reference_kind(
                Some("linkedin-outreach"),
                "claude",
                ("w4", Some("Consulting")),
                ("w4:t7", Some("linkedin-helper")),
                "w4:p7",
                AgentState::Working,
            ),
            reference_kind(
                Some("herdr_references"),
                "codex",
                ("wJ", Some("herdr-reference-discovery")),
                ("wJ:t1", Some("1")),
                "wJ:p1",
                AgentState::Working,
            ),
        ],
    );

    let choices = app.invocation_view().expect("picker").1;
    assert_eq!(choices[0].token, "codex");
    assert_eq!(choices[0].qualifier, "Meta · p1 · idle");
    assert_eq!(choices[1].token, "coaching-philipp");
    assert_eq!(choices[1].qualifier, "Consulting · p2 · claude · idle");
    assert_eq!(choices[2].token, "linkedin-outreach");
    assert_eq!(
        choices[2].qualifier,
        "Consulting / linkedin-helper · p7 · claude · working"
    );
    assert_eq!(choices[3].token, "herdr_references");
    assert_eq!(
        choices[3].qualifier,
        "herdr-reference-discovery · p1 · codex · working"
    );
    assert_eq!(
        choices
            .iter()
            .filter(|choice| choice.group.is_some())
            .count(),
        1
    );
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

fn install_catalog(
    app: &mut crate::ui::BoardApp,
    cwd: &Path,
    project: Vec<InvocationEntry>,
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
    }));
    generation
}

fn complete_live(
    app: &mut crate::ui::BoardApp,
    effects: &[Effect],
    references: Result<Vec<LiveAgentReference>, crate::ports::agent::AgentFailureCode>,
) -> u64 {
    let request = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::DiscoverInvocationReferences(request) => Some(request),
            _ => None,
        })
        .expect("live refresh effect");
    app.complete_invocation_reference_discovery(InvocationReferenceDiscovery {
        generation: request.generation,
        references,
    });
    request.generation
}

fn open_with_live(app: &mut crate::ui::BoardApp, live: Vec<LiveAgentReference>) -> u64 {
    let effects = app.open_invocation_picker();
    complete_live(app, &effects, Ok(live))
}

#[test]
fn manual_picker_keeps_installed_entries_and_truthfully_groups_duplicate_agent_names() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("prompt", cwd.path());
    install_catalog(
        &mut app,
        cwd.path(),
        vec![entry(
            "$review",
            crate::ports::invocation::InvocationKind::Skill,
            crate::ports::invocation::InvocationScope::Project,
        )],
    );
    open_with_live(
        &mut app,
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

    let choices = app.invocation_view().expect("picker").1;
    assert_eq!(choices.len(), 4);
    assert_eq!(choices[0].token, "$review");
    assert_eq!(choices[1].token, "reviewer");
    assert_eq!(
        choices[1].qualifier,
        "Product / Review · p9 · codex · working"
    );
    assert_eq!(choices[1].group.as_deref(), Some("Live in Herdr"));
    assert_eq!(choices[2].group, None);
    assert_eq!(choices[3].group, None);
    assert_eq!(choices[3].qualifier, "w3 / t2 · p1 · codex · idle");
}

#[test]
fn automatic_selection_inserts_an_inert_location_and_readiness_is_display_only() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("Ask @", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    let discovery = app.handle(UiInput::Key(UiKey::Character('r')), &mut ids, &clock);
    complete_live(
        &mut app,
        &discovery,
        Ok(vec![reviewer(AgentState::Working)]),
    );

    let choice = &app.invocation_view().expect("automatic picker").1[0];
    assert!(choice.qualifier.contains("working"));
    let selection_effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(selection_effects.is_empty());
    let inserted = "Ask Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 ";
    assert_eq!(app.editor_snapshot().expect("editor").content, inserted);
    assert!(!inserted.contains("working"));
    assert_eq!(
        app.editor_presentation()
            .expect("presentation")
            .snapshot
            .content,
        "Ask @reviewer · codex "
    );
    let thought_id = app.active_thought_id().expect("active thought");
    let annotations = app.current_annotations(thought_id);
    let [annotation] = annotations.as_slice() else {
        panic!("one reference annotation");
    };
    assert_eq!(
        &inserted[annotation.start..annotation.end],
        &inserted[4..inserted.len() - 1]
    );
    assert_eq!(
        annotation.kind,
        ContentAnnotationKind::InvocationReference {
            display_name: "@reviewer · codex".to_owned(),
        }
    );

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
    install_catalog(&mut app, cwd.path(), Vec::new());
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
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
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
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);

    assert_eq!(
        app.editor_snapshot().expect("editor").content,
        "Coordinate Herdr collaborator: reviewer (codex) at workspace Product (w2), tab Review (w2:t4), pane w2:p9 "
    );
}

#[test]
fn narrow_resized_mouse_geometry_selects_the_same_inert_reference() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(&mut app, vec![reviewer(AgentState::Idle)]);
    let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 30, 6));
    let overlay = layout.overlay.expect("overlay");
    let item = overlay.items[0];
    let heading = overlay.item_headings[0].expect("group heading");
    assert_eq!(item.height, 1);
    app.handle(
        UiInput::Pointer(PointerInput {
            column: heading.x,
            row: heading.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert!(app.invocation_view().is_some());
    assert_eq!(app.editor_snapshot().expect("editor").content, "");
    app.handle(
        UiInput::Pointer(PointerInput {
            column: item.x,
            row: item.y,
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
fn shallow_keyboard_navigation_keeps_the_active_live_group_visible() {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, mut ids, clock) = app("", cwd.path());
    install_catalog(
        &mut app,
        cwd.path(),
        vec![entry(
            "$review",
            crate::ports::invocation::InvocationKind::Skill,
            crate::ports::invocation::InvocationScope::Project,
        )],
    );
    open_with_live(
        &mut app,
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
    app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 30, 6));
    app.handle(UiInput::Key(UiKey::PickerNext), &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::PickerNext), &mut ids, &clock);

    let (_, choices, selected) = app.invocation_view().expect("scrolled picker");
    assert_eq!(choices[selected].token, "builder");
    assert_eq!(choices[0].group.as_deref(), Some("Live in Herdr"));
    let layout = app.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 30, 6));
    assert!(selected < layout.overlay.expect("overlay").items.len());
    app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(
        app.editor_snapshot()
            .expect("editor")
            .content
            .starts_with("Herdr collaborator: builder (codex)")
    );
}

#[path = "reference_tests/lifecycle.rs"]
mod lifecycle;
#[path = "reference_tests/mentions.rs"]
mod mentions;
#[path = "reference_tests/snapshots.rs"]
mod snapshots;

#[test]
fn live_group_snapshot_covers_wide_layout() {
    insta::assert_snapshot!("invocation_live_wide", snapshots::live_snapshot(72, 12));
}

#[test]
fn live_group_snapshot_covers_narrow_and_shallow_layout() {
    insta::assert_snapshot!(
        "invocation_live_narrow_shallow",
        snapshots::live_snapshot(30, 6)
    );
}
