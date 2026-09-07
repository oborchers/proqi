//! Commands keyboard/mouse parity and representative reflow discovery snapshots.

use super::{Fixture, key_input};
use crate::{draw_theme, snapshot_support::snapshot_buffer};
use proqi::ui::{PointerButton, PointerKind, ThemePreference, UiKey};
use ratatui_core::layout::Rect;

#[test]
fn commands_keyboard_and_mouse_reflow_the_same_focused_thought() {
    for mouse in [false, true] {
        let mut fixture = Fixture::new();
        fixture.paste("complete\nthought");
        fixture.input(key_input(UiKey::Escape));
        fixture.input(key_input(UiKey::Character(':')));
        for character in "reflow thought".chars() {
            fixture.input(key_input(UiKey::Character(character)));
        }
        assert_eq!(
            fixture.app.palette_view().expect("commands").1,
            ["Reflow thought in place"]
        );
        if mouse {
            let item = fixture
                .app
                .prepare_frame(Rect::new(0, 0, 60, 12))
                .overlay
                .expect("overlay")
                .items[0];
            fixture.pointer(item.x, item.y, PointerKind::Down(PointerButton::Left));
        } else {
            fixture.input(key_input(UiKey::Enter));
        }
        assert_eq!(
            fixture.app.state.board.live_thoughts()[0].content,
            "complete thought"
        );
        fixture.input(key_input(UiKey::Undo));
        assert_eq!(
            fixture.app.state.board.live_thoughts()[0].content,
            "complete\nthought"
        );
    }
}

#[test]
fn commands_mouse_from_edit_uses_editor_history_and_complete_content() {
    let mut fixture = Fixture::with_settings(proqi::ui::UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"commands.open\"=[{key='F5'}]",
        )
        .expect("keymap"),
        ..proqi::ui::UiSettings::default()
    });
    fixture.paste("complete\nthought");
    fixture.input(proqi::ui::UiInput::KeyStroke(proqi::ui::KeyStroke::press(
        proqi::ui::LogicalKey::Function(5),
    )));
    for character in "reflow thought".chars() {
        fixture.input(key_input(UiKey::Character(character)));
    }
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 80, 15))
        .overlay
        .expect("overlay")
        .items[0];
    fixture.pointer(item.x, item.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        fixture.app.editor_snapshot().expect("still edit").content,
        "complete thought"
    );
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "complete\nthought"
    );
}

#[test]
fn reflow_commands_discovery_snapshot() {
    let mut fixture = Fixture::new();
    fixture.paste("terminal copied\nprose");
    fixture.input(key_input(UiKey::Escape));
    fixture.input(key_input(UiKey::Character(':')));
    for character in "reflow".chars() {
        fixture.input(key_input(UiKey::Character(character)));
    }
    let terminal = draw_theme(&mut fixture, 60, 12, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}
