use super::*;

#[test]
fn remapped_board_binding_changes_behavior_and_visible_hint() {
    let mut settings = UiSettings::default();
    settings.keybindings.new = 't';
    let mut fixture = Fixture::with_settings(settings);
    fixture.input(UiInput::Key(UiKey::Character('n')));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Character('t')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(
        fixture.app.state.board.live_thoughts()[0]
            .content
            .is_empty()
    );
    fixture.input(UiInput::Key(UiKey::Escape));
    assert!(text(draw(&mut fixture, 50, 6).backend().buffer()).contains("t New"));
}

#[test]
fn explicit_web_urls_use_link_role_and_underline_without_changing_content() {
    let mut fixture = Fixture::new();
    let content = "See https://google.com? now";
    fixture.paste(content);
    fixture.input(UiInput::Key(UiKey::Escape));
    let terminal = draw_theme(&mut fixture, 60, 8, ThemePreference::Dark);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 60, 8)).thoughts[0].text_area;
    let theme = Theme::resolve(ThemePreference::Dark, true);
    let url = &terminal.backend().buffer()[(area.x + 4, area.y)];
    assert_eq!(url.fg, theme.link);
    assert!(
        url.modifier
            .contains(ratatui_core::style::Modifier::UNDERLINED)
    );
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}

#[test]
fn long_thought_cap_expands_without_changing_content() {
    let mut fixture = Fixture::new();
    let content = (0..8)
        .map(|index| format!("line {index} with enough ordinary words to wrap several times"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.paste(&content);
    fixture.input(UiInput::Key(UiKey::Escape));
    let initial = fixture.app.prepare_frame(Rect::new(0, 0, 40, 13));
    let thought = initial.thoughts.first().expect("thought");
    assert!(thought.hidden_rows > 0);
    let capped_height = thought.area.height;
    let overflow = thought.overflow.expect("overflow");
    let rendered = text(draw(&mut fixture, 40, 13).backend().buffer());
    assert!(rendered.contains(&format!("{} more lines", thought.hidden_rows)));
    fixture.pointer(
        overflow.x,
        overflow.y,
        PointerKind::Down(PointerButton::Left),
    );
    let expanded = fixture.app.prepare_frame(Rect::new(0, 0, 40, 13));
    assert!(expanded.thoughts[0].area.height > capped_height);
    assert!(expanded.thoughts[0].overflow.is_none());
    assert_eq!(expanded.thoughts[0].hidden_rows, 0);
    assert!(expanded.thoughts[0].viewport_clipped);
    assert!(expanded.thoughts[0].scrollable_hidden);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        proqi::domain::ThoughtPresentation::Expanded
    );
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}

#[test]
fn collapsed_keyboard_edit_entry_expands_before_opening_the_full_editor() {
    for key in [UiKey::Enter, UiKey::Character('e')] {
        let mut fixture = Fixture::new();
        let content = (0..10)
            .map(|index| format!("line {index} wraps with Grüße 界 and enough ordinary words"))
            .collect::<Vec<_>>()
            .join("\n");
        super::navigation::durable_thought(&mut fixture, &content);
        let area = Rect::new(0, 0, 38, 11);
        let _automatic = fixture.app.prepare_frame(area);
        fixture.input(UiInput::Key(UiKey::Character('c')));
        fixture.input(UiInput::Key(UiKey::Character('c')));
        let collapsed = fixture.app.prepare_frame(area);
        assert_eq!(collapsed.thoughts[0].area.height, 2);

        let effects = fixture.effects(UiInput::Key(key));
        assert!(matches!(
            effects.as_slice(),
            [Effect::CommitBoardOperation(operation)]
                if operation.kind == proqi::domain::BoardOperationKind::Collapse
        ));
        assert!(matches!(
            fixture.app.interaction_mode(),
            proqi::application::InteractionMode::Edit { .. }
        ));
        assert_eq!(
            fixture.app.state.board.live_thoughts()[0].presentation,
            proqi::domain::ThoughtPresentation::Expanded
        );
        let editing = fixture.app.prepare_frame(area);
        assert!(editing.thoughts[0].area.height > 2);
        assert!(editing.thoughts[0].overflow.is_none());
        assert_eq!(
            fixture.app.editor_snapshot().expect("editor").content,
            content
        );
    }
}

#[test]
fn collapsed_content_click_expands_and_maps_the_visible_wide_cell_before_editing() {
    let mut fixture = Fixture::new();
    let content = (0..10)
        .map(|index| format!("A界B line {index} with enough words to wrap twice"))
        .collect::<Vec<_>>()
        .join("\n");
    super::navigation::durable_thought(&mut fixture, &content);
    let area = Rect::new(0, 0, 36, 10);
    let _automatic = fixture.app.prepare_frame(area);
    fixture.input(UiInput::Key(UiKey::Character('c')));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    let collapsed = fixture.app.prepare_frame(area);
    let text_area = collapsed.thoughts[0].text_area;

    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: text_area.x + 3,
        row: text_area.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));

    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if operation.kind == proqi::domain::BoardOperationKind::Collapse
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        proqi::domain::ThoughtPresentation::Expanded
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, 2)
    );
    let editing = fixture.app.prepare_frame(area);
    assert!(editing.thoughts[0].area.height > 2);
    assert!(editing.thoughts[0].overflow.is_none());
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}

#[test]
fn collapsed_gutter_click_expands_without_starting_a_stale_drag() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(
        &mut fixture,
        &(0..10)
            .map(|line| format!("long gutter line {line} that wraps"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let area = Rect::new(0, 0, 34, 10);
    let _automatic = fixture.app.prepare_frame(area);
    fixture.input(UiInput::Key(UiKey::Character('c')));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    let collapsed = fixture.app.prepare_frame(area);
    let gutter = collapsed.thoughts[0].gutter;

    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: gutter.x,
        row: gutter.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));

    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        proqi::domain::ThoughtPresentation::Expanded
    );
    assert!(fixture.app.dragged_thought().is_none());
    assert!(
        fixture.app.prepare_frame(area).thoughts[0]
            .overflow
            .is_none()
    );
}

#[test]
fn viewport_matrix_keeps_focus_visible_and_hit_geometry_current() {
    let mut fixture = Fixture::new();
    for index in 0..10 {
        fixture.paste(&format!("thought {index} 界"));
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let focused = fixture.app.state.focused_thought.expect("focus");
    for (width, height) in [(6, 3), (120, 4), (18, 30), (9, 5), (80, 24)] {
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, height));
        let thought = layout.thought(focused).expect("focused thought visible");
        assert!(thought.area.right() <= width);
        assert!(thought.area.bottom() <= layout.board.bottom());
        assert_eq!(
            layout.hit_test(thought.gutter.x, thought.gutter.y),
            Some(proqi::ui::HitTarget::DragHandle(focused))
        );
        if thought.text_area.width > 0 {
            assert!(matches!(
                layout.hit_test(thought.text_area.x, thought.text_area.y),
                Some(proqi::ui::HitTarget::Thought(id) | proqi::ui::HitTarget::Overflow(id))
                    if id == focused
            ));
        }
    }
}

#[test]
fn focused_surface_keeps_text_foreground_and_uses_neutral_background() {
    let mut fixture = Fixture::new();
    fixture.paste("selected text");
    let terminal = draw_theme(&mut fixture, 40, 8, ThemePreference::Light);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    let cell = &terminal.backend().buffer()[layout.thoughts[0].text_area.as_position()];
    let theme = Theme::resolve(ThemePreference::Light, true);
    assert_eq!(cell.fg, theme.foreground);
    assert_eq!(cell.bg, theme.focused_surface.expect("explicit surface"));
    assert_ne!(cell.fg, theme.accent);
}

#[test]
fn chrome_and_thought_rhythm_are_responsive_and_non_overlapping() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let roomy = fixture.app.prepare_frame(Rect::new(0, 0, 60, 14));
    assert_eq!(roomy.thoughts[0].area.y, roomy.board.y + 1);
    let rule = roomy.thoughts[1].separator_before.expect("roomy rule");
    assert_eq!(rule.y, roomy.thoughts[0].area.bottom());
    assert_eq!(roomy.thoughts[1].area.y, rule.bottom() + 1);
    assert!(roomy.header.bottom() <= roomy.board.y);
    assert!(roomy.board.bottom() <= roomy.footer.y);
    assert!(roomy.footer_context.bottom() <= roomy.footer_actions.y);

    let shallow = fixture.app.prepare_frame(Rect::new(0, 0, 60, 6));
    assert_eq!(shallow.thoughts[0].area.y, shallow.board.y);
    let rule = shallow.thoughts[1].separator_before.expect("compact rule");
    assert_eq!(rule.y, shallow.thoughts[0].area.bottom());
    assert_eq!(shallow.thoughts[1].area.y, rule.bottom());
}

#[test]
fn long_session_names_remain_visible_and_renameable_after_resize() {
    let mut fixture = Fixture::new();
    fixture
        .app
        .state
        .board
        .session
        .rename(Some(
            "aia-knowledge-platform-interface-with-a-long-context".to_owned(),
        ))
        .expect("session name");

    for width in [80, 48, 24] {
        let terminal = draw(&mut fixture, width, 8);
        let rendered = text(terminal.backend().buffer());
        assert!(rendered.contains("aia-knowledge"));
        assert!(rendered.contains("saved"));
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, 8));
        assert!(layout.footer_name.height > 0);
        assert!(
            layout
                .controls
                .iter()
                .any(|(target, area)| { *target == HitTarget::RenameSession && area.width > 0 })
        );
    }
}

#[test]
fn multiline_edit_at_the_end_reflows_and_keeps_the_cursor_visible() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));
    let active = fixture.app.active_thought_id().expect("new thought editor");
    let before = fixture.app.prepare_frame(Rect::new(0, 0, 34, 12));
    assert_eq!(
        before.thought(active).expect("blank thought").area.height,
        1
    );

    let content = (1..=8)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.input(UiInput::Paste(content));
    let mut terminal = draw(&mut fixture, 34, 12);
    let after = fixture.app.prepare_frame(Rect::new(0, 0, 34, 12));
    let thought = after
        .thought(active)
        .expect("edited thought remains visible");
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("visible terminal cursor");
    assert!(thought.area.height > 1);
    assert!(cursor.y >= thought.text_area.y && cursor.y < thought.text_area.bottom());
    assert!(text(terminal.backend().buffer()).contains("line 8"));
}

#[test]
fn narrow_empty_board_has_a_complete_explicit_buffer_snapshot() {
    let mut fixture = Fixture::new();
    assert_eq!(
        text(draw(&mut fixture, 12, 3).backend().buffer()),
        "+ New though\n  0 saved   \n  n New     "
    );
}
