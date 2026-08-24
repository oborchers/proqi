use super::*;

fn snapshot(fixture: &mut Fixture, width: u16, height: u16, theme: ThemePreference) -> String {
    let terminal = draw_theme(fixture, width, height, theme);
    let buffer = terminal.backend().buffer();
    format!(
        "TEXT\n{}\n\nSTYLE RUNS\n{}",
        text(buffer),
        style_runs(buffer)
    )
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
