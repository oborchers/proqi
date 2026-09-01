//! First-run board rendering, semantic shortcut emphasis, and canonical URL styling.

use proqi::{
    application::FirstRunEnvironment,
    ui::{Theme, ThemePreference, UiInput, UiKey},
};
use ratatui_core::style::Modifier;

use super::{Fixture, draw_theme, snapshot_support::snapshot_buffer};

fn snapshot(
    environment: FirstRunEnvironment,
    width: u16,
    height: u16,
    navigation: usize,
) -> String {
    let mut fixture = Fixture::first_run(environment);
    for _ in 0..navigation {
        fixture.input(UiInput::Key(UiKey::Character('j')));
    }
    let terminal = draw_theme(&mut fixture, width, height, ThemePreference::Dark);
    snapshot_buffer(terminal.backend().buffer())
}

#[test]
fn managed_practice_board_is_reviewed_at_narrow_and_shallow_sizes() {
    insta::assert_snapshot!(
        "managed_practice_board_narrow",
        snapshot(FirstRunEnvironment::HerdrManaged, 34, 12, 4,)
    );
    insta::assert_snapshot!(
        "managed_practice_board_shallow",
        snapshot(FirstRunEnvironment::HerdrManaged, 80, 6, 5,)
    );
}

#[test]
fn standalone_practice_board_is_reviewed_at_standard_and_wide_sizes() {
    insta::assert_snapshot!(
        "standalone_practice_board_standard",
        snapshot(FirstRunEnvironment::Standalone, 72, 18, 4,)
    );
    insta::assert_snapshot!(
        "standalone_practice_board_wide",
        snapshot(FirstRunEnvironment::Standalone, 120, 30, 5,)
    );
}

#[test]
fn canonical_herdr_url_uses_existing_link_styling_without_content_changes() {
    let mut fixture = Fixture::first_run(FirstRunEnvironment::HerdrManaged);
    let original_content = fixture.app.state.board.live_thoughts()[4].content.clone();
    for _ in 0..4 {
        fixture.input(UiInput::Key(UiKey::Character('j')));
    }
    let terminal = draw_theme(&mut fixture, 120, 20, ThemePreference::Dark);
    let buffer = terminal.backend().buffer();
    let url = "https://herdr.dev";
    let (row, start) = (0..buffer.area.height)
        .find_map(|row| {
            let text = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            text.find(url).map(|start| (row, start))
        })
        .expect("visible canonical URL");
    let theme = Theme::resolve(ThemePreference::Dark, true);
    for offset in 0..url.len() {
        let column = u16::try_from(start + offset).expect("URL within terminal width");
        let cell = &buffer[(column, row)];
        assert_eq!(cell.fg, theme.link);
        assert!(cell.modifier.contains(Modifier::UNDERLINED));
    }
    assert_eq!(
        fixture.app.state.board.live_thoughts()[4].content,
        original_content
    );
}
