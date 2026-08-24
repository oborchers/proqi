use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentReadiness, AgentTarget, PaneContext, PaneRect},
};

fn snapshot(fixture: &mut Fixture, width: u16, height: u16, theme: ThemePreference) -> String {
    let terminal = draw_theme(fixture, width, height, theme);
    let buffer = terminal.backend().buffer();
    format!(
        "SIZE {}x{}\n\nTEXT\n{}\n\nSTYLE RUNS\n{}",
        buffer.area.width,
        buffer.area.height,
        snapshot_text(buffer),
        style_runs(buffer)
    )
}

fn snapshot_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            let row = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            format!("{y:02}│{}│", row.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn style_runs(buffer: &Buffer) -> String {
    let mut rows = Vec::new();
    for y in 0..buffer.area.height {
        let mut runs = Vec::new();
        let mut start = 0;
        let mut previous = style_key(buffer, 0, y);
        for x in 1..buffer.area.width {
            let current = style_key(buffer, x, y);
            if current != previous {
                runs.push(format!("{start}-{end} {previous}", end = x - 1));
                start = x;
                previous = current;
            }
        }
        runs.push(format!(
            "{start}-{end} {previous}",
            end = buffer.area.width.saturating_sub(1)
        ));
        rows.push(format!("{y}: {}", runs.join(" | ")));
    }
    rows.join("\n")
}

fn style_key(buffer: &Buffer, x: u16, y: u16) -> String {
    let cell = &buffer[(x, y)];
    format!("fg={:?} bg={:?} mod={:?}", cell.fg, cell.bg, cell.modifier)
}

fn adjacent_target(direction: Direction, pane_id: &str, readiness: AgentReadiness) -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 40,
            y: 20,
            width: 40,
            height: 20,
        },
    };
    AgentTarget {
        direction,
        pane_id: pane_id.to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: "codex".to_owned(),
        agent_name: format!("Codex {pane_id}"),
        agent_session_id: format!("session-{pane_id}"),
        readiness,
        rect: source.rect,
        source,
    }
}

#[test]
fn empty_and_narrow_board() {
    let mut fixture = Fixture::new();
    insta::assert_snapshot!(snapshot(&mut fixture, 24, 6, ThemePreference::Dark));
}

#[test]
fn populated_board_with_folded_attachment() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Paste("first prompt".to_owned()));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        "/private/tmp/screenshot.png".to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: "/private/tmp/screenshot.png".len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "screenshot.png".to_owned(),
            },
        }],
    )));
    fixture.input(UiInput::Key(UiKey::Escape));
    insta::assert_snapshot!(snapshot(&mut fixture, 60, 12, ThemePreference::Dark));
}

#[test]
fn ephemeral_draft_and_editing_surface() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character('n')));
    insta::assert_snapshot!(snapshot(&mut fixture, 50, 9, ThemePreference::Light));
}

#[test]
fn failed_save_uses_a_dedicated_status_row() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("important prompt");
    fixture.app.acknowledge_persistence(sequence, false);
    insta::assert_snapshot!(snapshot(&mut fixture, 55, 9, ThemePreference::Dark));
}

#[test]
fn help_overlay_remains_composed_in_a_shallow_viewport() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character('?')));
    insta::assert_snapshot!(snapshot(&mut fixture, 42, 8, ThemePreference::Auto));
}

#[test]
fn limited_color_mode_uses_terminal_palette_styles() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Paste("selected prompt".to_owned()));
    insta::assert_snapshot!(snapshot(&mut fixture, 48, 8, ThemePreference::Limited));
}

#[test]
fn four_direction_agent_controls_have_a_dedicated_footer_band() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Paste("selected prompt".to_owned()));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.app.complete_agent_discovery(Ok(vec![
        adjacent_target(Direction::Up, "w1:p2", AgentReadiness::Idle),
        adjacent_target(Direction::Right, "w1:p3", AgentReadiness::Working),
        adjacent_target(Direction::Down, "w1:p4", AgentReadiness::Done),
        adjacent_target(Direction::Left, "w1:p5", AgentReadiness::Idle),
    ]));
    insta::assert_snapshot!(snapshot(&mut fixture, 120, 12, ThemePreference::Dark));
}

#[test]
fn drag_preview_uses_the_existing_separator_without_reflow() {
    let mut fixture = Fixture::new();
    for content in ["first thought", "second thought", "third thought"] {
        fixture.input(UiInput::Paste(content.to_owned()));
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let _initial = draw(&mut fixture, 60, 14);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 60, 14));
    fixture.pointer(
        layout.thoughts[0].gutter.x,
        layout.thoughts[0].gutter.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(
        layout.thoughts[2].gutter.x,
        layout.thoughts[2].gutter.y,
        PointerKind::Drag(PointerButton::Left),
    );
    insta::assert_snapshot!(snapshot(&mut fixture, 60, 14, ThemePreference::Dark));
}

#[test]
fn long_paste_stays_folded_and_accented_in_edit_mode() {
    let mut fixture = Fixture::new();
    let content = (0..84)
        .map(|line| format!("long context line {line:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content));
    insta::assert_snapshot!(snapshot(&mut fixture, 72, 9, ThemePreference::Dark));
}
