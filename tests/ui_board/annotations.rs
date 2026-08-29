use super::*;
use proptest::prelude::*;

fn image_payload(path: &str) -> PastePayload {
    attachment_payload(path, true)
}

fn attachment_payload(path: &str, image: bool) -> PastePayload {
    PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image,
                display_name: "screenshot.png".to_owned(),
            },
        }],
    )
}

#[test]
fn image_path_folds_immediately_but_every_exact_content_path_is_preserved() {
    let mut fixture = Fixture::new();
    let path = "/private/temporary/location/screenshot.png";
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, path);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].annotations.len(),
        1
    );

    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Image 1]"));
    assert!(!rendered.contains("screenshot.png"));
    assert!(!rendered.contains("/private/temporary"));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer())
            .contains("/private/temporary/location/screenshot.png")
    );
    fixture.input(UiInput::Key(UiKey::Escape));

    let effects = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        effects.as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == path
    ));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, path);
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Image 1]"));
}

#[test]
fn files_use_the_minimal_accent_placeholder_without_exposing_the_path() {
    let mut fixture = Fixture::new();
    let path = "/private/temporary/location/context.pdf";
    fixture.input(UiInput::PasteAnnotated(attachment_payload(path, false)));
    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("[File 1]"));
    assert!(!rendered.contains("context.pdf"));
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    let cell = &terminal.backend().buffer()[(area.x, area.y)];
    assert_eq!(
        cell.fg,
        Theme::resolve(ThemePreference::Dark, true).annotation
    );
}

#[test]
fn large_paste_is_folded_while_editing_and_editor_undo_restores_its_fold() {
    let mut fixture = Fixture::new();
    let content = (0..14)
        .map(|line| format!("context line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content.clone()));
    let rendered = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(rendered.contains("[Pasted text · 14 lines · 213 characters]"));
    assert!(!rendered.contains("context line 13"));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));
    let expanded = text(draw(&mut fixture, 60, 8).backend().buffer());
    assert!(expanded.contains("context line 13"));
    assert!(!expanded.contains("[Pasted text"));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let effects = fixture.effects(UiInput::Key(UiKey::Undo));
    assert_eq!(effects.len(), 2);
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
    assert!(
        text(draw(&mut fixture, 60, 8).backend().buffer()).contains("[Pasted text · 14 lines ·")
    );
}

#[test]
fn fast_and_boundary_navigation_keep_a_collapsed_annotation_atomic() {
    let content = (0..10)
        .map(|row| format!("folded row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let graphemes =
        unicode_segmentation::UnicodeSegmentation::graphemes(content.as_str(), true).count();
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        content.clone(),
        vec![ContentAnnotation {
            start: 0,
            end: content.len(),
            kind: ContentAnnotationKind::LargePaste {
                lines: 10,
                graphemes,
            },
        }],
    )));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, 0)
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualJumpDown,
        extend_selection: false,
    }));
    let selected = fixture.app.editor_snapshot().expect("fold selection");
    assert_eq!(
        selected.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(9, "folded row 9".chars().count()),
        })
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));
    let end = fixture.app.editor_snapshot().expect("thought end");
    assert_eq!(end.selection, None);
    assert_eq!(end.cursor, proqi::domain::TextPosition::new(9, 12));
}

#[test]
fn collapsed_folds_are_atomic_for_selection_replacement_and_expansion() {
    let mut fixture = Fixture::new();
    let path = "/tmp/screenshot.png";
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(
        snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(0, path.len()),
        })
    );
    let terminal = draw(&mut fixture, 40, 8);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    for column in 0.."[Image 1]".len() {
        let column = u16::try_from(column).expect("short placeholder");
        assert!(
            terminal.backend().buffer()[(area.x + column, area.y)]
                .modifier
                .contains(ratatui_core::style::Modifier::REVERSED)
        );
    }
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let before = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(before.cursor, proqi::domain::TextPosition::new(0, 0));
    assert!(before.selection.is_none());
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .selection
            .is_some()
    );
    fixture.input(UiInput::Key(UiKey::Character('x')));
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, "x");

    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(path)));
    let _rendered = draw(&mut fixture, 40, 8);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    fixture.pointer(
        area.x.saturating_add(2),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(0, path.len()),
        })
    );
    assert!(text(draw(&mut fixture, 40, 8).backend().buffer()).contains("[Image 1]"));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert!(text(draw(&mut fixture, 40, 8).backend().buffer()).contains(path));

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Backspace));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .content
            .is_empty()
    );
}

#[test]
fn folded_editor_keeps_a_visible_terminal_cursor_at_the_token_boundary() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(
        "/tmp/screenshot.png",
    )));
    let mut terminal = draw(&mut fixture, 40, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("visible cursor");
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    assert_eq!((cursor.x, cursor.y), (area.x + 9, area.y));
}

#[test]
fn folded_cursor_projects_before_selected_and_after_without_extra_steps() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(
        "/tmp/screenshot.png",
    )));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("selected fold")
            .selection
            .is_some()
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    let mut before = draw(&mut fixture, 40, 8);
    let cursor = before
        .backend_mut()
        .get_cursor_position()
        .expect("cursor before fold");
    assert_eq!((cursor.x, cursor.y), (area.x, area.y));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("selected fold")
            .selection
            .is_some()
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    let mut after = draw(&mut fixture, 40, 8);
    let cursor = after
        .backend_mut()
        .get_cursor_position()
        .expect("cursor after fold");
    assert_eq!((cursor.x, cursor.y), (area.x + 9, area.y));
}

#[test]
fn reverse_fold_navigation_uses_the_visible_space_before_an_inline_placeholder() {
    let path = "/tmp/screenshot.png";
    let prefix = "before ";
    let suffix = " after";
    let content = format!("{prefix}{path}{suffix}");
    let start = prefix.len();
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        content,
        vec![ContentAnnotation {
            start,
            end: start + path.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "screenshot.png".to_owned(),
            },
        }],
    )));
    for _ in 0..suffix.chars().count() {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
    }
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("fold")
            .selection
            .is_some()
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 50, 8)).thoughts[0].text_area;
    let mut terminal = draw(&mut fixture, 50, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor before fold");
    assert_eq!((cursor.x, cursor.y), (area.x + 6, area.y));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let mut terminal = draw(&mut fixture, 50, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor on preceding grapheme");
    assert_eq!((cursor.x, cursor.y), (area.x + 5, area.y));
}

#[test]
fn adjacent_folds_remain_independently_atomic() {
    let first = "/tmp/first.png";
    let second = "/tmp/second.png";
    let content = format!("{first}{second}");
    let split = first.len();
    let payload = PastePayload::annotated(
        content,
        vec![
            ContentAnnotation {
                start: 0,
                end: split,
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "first.png".to_owned(),
                },
            },
            ContentAnnotation {
                start: split,
                end: split + second.len(),
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "second.png".to_owned(),
                },
            },
        ],
    );
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(payload));

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("second fold")
            .selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, split),
            end: proqi::domain::TextPosition::new(0, split + second.len()),
        })
    );
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert_eq!(
        fixture.app.editor_snapshot().expect("first fold").selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 0),
            end: proqi::domain::TextPosition::new(0, split),
        })
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn collapsed_fold_navigation_never_leaves_a_cursor_inside_hidden_content(
        forwards in proptest::collection::vec(any::<bool>(), 0..80),
    ) {
        let path = "/tmp/atomic-hidden-image.png";
        let mut fixture = Fixture::new();
        fixture.input(UiInput::PasteAnnotated(image_payload(path)));
        for forward in forwards {
            fixture.input(UiInput::Key(UiKey::Move {
                movement: if forward {
                    CursorMovement::GraphemeForward
                } else {
                    CursorMovement::GraphemeBack
                },
                extend_selection: false,
            }));
            let snapshot = fixture.app.editor_snapshot().expect("editor");
            if let Some(selection) = snapshot.selection {
                prop_assert_eq!(
                    selection,
                    proqi::ports::editor::TextSelection {
                        start: proqi::domain::TextPosition::new(0, 0),
                        end: proqi::domain::TextPosition::new(0, path.len()),
                    }
                );
            } else {
                prop_assert!(
                    snapshot.cursor == proqi::domain::TextPosition::new(0, 0)
                        || snapshot.cursor
                            == proqi::domain::TextPosition::new(0, path.len())
                );
            }
        }
    }
}

#[test]
fn folded_tokens_use_the_annotation_role_and_bold_non_color_cue() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(image_payload(
        "/tmp/screenshot.png",
    )));
    let terminal = draw_theme(&mut fixture, 40, 8, ThemePreference::Dark);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    let text = layout.thoughts[0].text_area;
    let cell = &terminal.backend().buffer()[(text.x, text.y)];
    let theme = Theme::resolve(ThemePreference::Dark, true);
    assert_eq!(cell.fg, theme.annotation);
    assert!(cell.modifier.contains(ratatui_core::style::Modifier::BOLD));
}
