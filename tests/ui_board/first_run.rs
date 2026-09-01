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
fn editing_thought_demonstrates_line_and_sentence_deletion_at_its_initial_cursor() {
    let first_line = "Press Enter to edit the focused thought. Press Esc to return to board mode.";
    let deletion_line = "- Press Enter to continue this unordered list. Press Primary+U to delete this logical line. Press Primary+Shift+U to delete this sentence.";

    let mut line_fixture = Fixture::first_run(FirstRunEnvironment::Standalone);
    line_fixture.input(UiInput::Key(UiKey::Character('j')));
    line_fixture.input(UiInput::Key(UiKey::Enter));
    line_fixture.input(UiInput::Key(UiKey::DeleteLogicalLine));
    assert_eq!(
        line_fixture
            .app
            .editor_snapshot()
            .expect("editing thought")
            .content,
        first_line
    );

    let mut sentence_fixture = Fixture::first_run(FirstRunEnvironment::Standalone);
    sentence_fixture.input(UiInput::Key(UiKey::Character('j')));
    sentence_fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(
        sentence_fixture
            .app
            .editor_snapshot()
            .expect("editing thought")
            .content,
        format!("{first_line}\n{deletion_line}")
    );
    sentence_fixture.input(UiInput::Key(UiKey::DeleteSentence));
    assert_eq!(
        sentence_fixture
            .app
            .editor_snapshot()
            .expect("editing thought")
            .content,
        format!(
            "{first_line}\n- Press Enter to continue this unordered list. Press Primary+U to delete this logical line."
        )
    );
}

#[test]
fn editing_thought_continues_its_unordered_list_at_the_initial_cursor() {
    let mut fixture = Fixture::first_run(FirstRunEnvironment::Standalone);
    fixture.input(UiInput::Key(UiKey::Character('j')));
    fixture.input(UiInput::Key(UiKey::Enter));
    let before = fixture
        .app
        .editor_snapshot()
        .expect("editing thought")
        .content;

    fixture.input(UiInput::Key(UiKey::Enter));

    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("continued unordered list")
            .content,
        format!("{before}\n- ")
    );
}

#[test]
fn canonical_herdr_url_uses_existing_link_styling_without_content_changes() {
    let mut fixture = Fixture::first_run(FirstRunEnvironment::HerdrManaged);
    let original_content = fixture.app.state.board.live_thoughts()[4].content.clone();
    for _ in 0..4 {
        fixture.input(UiInput::Key(UiKey::Character('j')));
    }
    let terminal = draw_theme(&mut fixture, 120, 30, ThemePreference::Dark);
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
