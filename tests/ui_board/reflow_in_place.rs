//! Whole-thought reflow scope, revision boundaries, and exact-paste regression.

use super::{Fixture, draw, key_input, text};
use proqi::{
    application::{Effect, InteractionMode},
    domain::{ContentAnnotationKind, TextPosition},
    ports::editor::CursorMovement,
    ui::{KeyStroke, LogicalKey, LogicalModifiers, UiInput, UiKey},
};

fn reflow(fixture: &mut Fixture) -> Vec<Effect> {
    let modifiers = if matches!(fixture.app.interaction_mode(), InteractionMode::Edit { .. }) {
        LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT)
    } else {
        LogicalModifiers::NONE
    };
    fixture.effects(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Character('f')).with_modifiers(modifiers),
    ))
}

#[test]
fn board_reflows_only_focus_and_retains_neighbors_and_range() {
    let mut fixture = Fixture::new();
    for content in ["neighbor\nexact", "focused  prose", "other\nexact"] {
        fixture.paste(content);
        fixture.input(key_input(UiKey::Escape));
    }
    fixture.input(key_input(UiKey::Character('k')));
    fixture.input(key_input(UiKey::Character('v')));
    let selected: Vec<_> = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| (thought.id, fixture.app.thought_selected(thought.id)))
        .collect();
    let effects = reflow(&mut fixture);
    assert!(
        matches!(effects.as_slice(), [Effect::CommitBoardOperation(op)] if op.kind == proqi::domain::BoardOperationKind::Reflow)
    );
    let thoughts = fixture.app.state.board.live_thoughts();
    assert_eq!(
        thoughts
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["neighbor\nexact", "focused prose", "other\nexact"]
    );
    for (id, before) in selected {
        assert_eq!(fixture.app.thought_selected(id), before);
    }
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[1].content,
        "focused  prose"
    );
    fixture.input(key_input(UiKey::Redo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[1].content,
        "focused prose"
    );
    assert!(reflow(&mut fixture).is_empty());
    assert_eq!(fixture.app.status_text(), Some("spacing already clean"));
}

#[test]
fn edit_reflows_complete_content_and_projects_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("first  line\nsecond");
    fixture.input(key_input(UiKey::Character('!')));
    fixture.input(key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: true,
    }));
    let effects = reflow(&mut fixture);
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    let after = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(after.content, "first line\nsecond!");
    assert_eq!(after.cursor, TextPosition::default());
    assert!(after.selection.is_some());
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "first  line\nsecond!"
    );
    fixture.input(key_input(UiKey::Redo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "first line\nsecond!"
    );
    assert!(reflow(&mut fixture).is_empty());
    fixture.input(key_input(UiKey::Character('f')));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .content
            .contains('f')
    );
}

#[test]
fn empty_whitespace_and_protected_thoughts_remain_exact() {
    for source in [
        "",
        " \r\n\t\r\n",
        "a\u{7}b\nline",
        "https://example.com\nexact",
        "    code\n    exact",
        "> quote\n> exact",
        "| a | b |\n| c | d |",
    ] {
        let mut fixture = Fixture::new();
        fixture.paste("neighbor");
        fixture.input(key_input(UiKey::Escape));
        fixture.input(key_input(UiKey::Character('n')));
        if !source.is_empty() {
            fixture.paste(source);
        }
        for board in [false, true] {
            if board {
                fixture.input(key_input(UiKey::Escape));
            }
            assert!(reflow(&mut fixture).is_empty(), "{source:?}");
            if let Some(thought) = fixture.app.state.board.live_thoughts().last() {
                assert_eq!(thought.content, source);
            }
        }
    }
}

#[test]
fn collapsed_large_paste_reflows_without_expansion_and_recounts() {
    let mut fixture = Fixture::new();
    let source = format!("{}\n  tail", "界e\u{301}👩🏽‍💻 ".repeat(400));
    fixture.paste(&source);
    assert!(text(draw(&mut fixture, 40, 8).backend().buffer()).contains("[Pasted text"));
    assert!(matches!(
        reflow(&mut fixture).as_slice(),
        [Effect::CommitRevision(_)]
    ));
    let thought = fixture.app.state.board.live_thoughts()[0];
    assert_eq!(
        thought.content,
        format!("{}\ntail", "界e\u{301}👩🏽‍💻 ".repeat(400).trim_end())
    );
    assert!(matches!(
        thought.annotations[0].kind,
        ContentAnnotationKind::LargePaste {
            lines: 2,
            graphemes: 1604
        }
    ));
    for (width, height) in [(25, 5), (80, 30), (30, 6)] {
        assert!(text(draw(&mut fixture, width, height).backend().buffer()).contains("Pasted"));
    }
    assert!(reflow(&mut fixture).is_empty());
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        source
    );
}

#[test]
fn line_threshold_fold_dissolves_and_lists_keep_semantics_for_lf_and_crlf() {
    for newline in ["\n", "\r\n"] {
        let mut fixture = Fixture::new();
        let source = [
            "  prose  ",
            "wraps",
            "",
            "- item",
            "  continuation",
            "- next",
            "",
            "",
            "",
            "",
            "",
            "tail",
        ]
        .join(newline);
        fixture.paste(&source);
        assert_eq!(
            fixture.app.state.board.live_thoughts()[0].annotations.len(),
            1
        );
        reflow(&mut fixture);
        let thought = fixture.app.state.board.live_thoughts()[0];
        assert_eq!(
            thought.content,
            format!(
                "prose{newline}wraps{newline}{newline}- item{newline}  continuation{newline}- next{newline}{newline}tail"
            )
        );
        assert!(thought.annotations.is_empty());
        assert!(reflow(&mut fixture).is_empty());
    }
}

#[test]
fn pending_typing_flushes_before_one_reflow_revision() {
    let mut fixture = Fixture::new();
    fixture.paste("first  line\nsecond");
    fixture.input(key_input(UiKey::Character('!')));
    let effects = reflow(&mut fixture);
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitRevision(_), Effect::CommitRevision(_)]
    ));
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "first  line\nsecond!"
    );
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "first  line\nsecond"
    );
}

#[path = "reflow_in_place/annotations.rs"]
mod annotations;

#[path = "reflow_in_place/commands.rs"]
mod commands;

#[path = "reflow_in_place/projection.rs"]
mod projection;
