use super::*;
use proqi::ui::FastNavigation;

#[test]
fn browser_fast_navigation_skips_recency_headings_and_stays_visible_after_resize() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let items = (0..12)
        .map(|index| {
            item(
                &mut ids,
                Some(&format!("Session {index}")),
                test_path(&format!("session-{index}")),
                &format!("preview {index}"),
                900_000_000 - i64::from(index) * 90_000_000,
                resumable,
            )
        })
        .collect::<Vec<_>>();
    let expected = items[5].hit.id;
    let mut browser = SessionBrowser::new(items, Timestamp::from_millis(900_000_000));
    let initial = draw(&mut browser, 38, 7);
    assert!(text(initial.backend().buffer()).contains('↓'));
    assert_eq!(
        browser.handle(UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        })),
        BrowserAction::Continue
    );
    assert_eq!(
        browser.selected_item().map(|(_, item)| item.hit.id),
        Some(expected)
    );
    let layout = browser.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 28, 5));
    assert!(layout.entries.iter().any(|entry| entry.item_index == 5));

    for _ in 0..4 {
        browser.handle(UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }));
    }
    assert_eq!(browser.selected_item().map(|(index, _)| index), Some(11));
}

#[test]
fn narrow_browser_input_keeps_sanitized_cursor_suffix_visible() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let entry = item(
        &mut ids,
        Some("input geometry"),
        test_path("input-geometry"),
        "matching content",
        10,
        resumable,
    );
    let mut browser = SessionBrowser::new(vec![entry], Timestamp::from_millis(20));
    browser.handle(UiInput::Paste("prefix\t界e\u{301}👩‍💻\u{7}tail".to_owned()));

    let rendered = draw(&mut browser, 18, 6);
    let header = text(rendered.backend().buffer())
        .lines()
        .nth(1)
        .expect("search header")
        .to_owned();
    assert!(header.trim_end().ends_with('_'));
    assert!(!header.contains(['\t', '\u{7}']));

    let mut rename = SessionBrowser::new(Vec::new(), Timestamp::from_millis(20));
    rename.handle(UiInput::Key(UiKey::Character('R')));
    rename.handle(UiInput::Paste("rename\t界👩‍💻\u{7}tail".to_owned()));
    let rendered = draw(&mut rename, 18, 6);
    let header = text(rendered.backend().buffer())
        .lines()
        .nth(1)
        .expect("rename header")
        .to_owned();
    assert!(header.trim_end().ends_with('_'));
    assert!(!header.contains(['\t', '\u{7}']));
}

#[test]
fn one_row_browser_body_keeps_the_selected_entry_instead_of_its_heading() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let entry = item(
        &mut ids,
        Some("shallow selection"),
        test_path("shallow-selection"),
        "visible result",
        10,
        resumable,
    );
    let mut browser = SessionBrowser::new(vec![entry], Timestamp::from_millis(20));

    let layout = browser.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 38, 4));
    assert_eq!(layout.results.height, 1);
    assert_eq!(layout.entries.len(), 1);
    assert!(layout.entries[0].group.is_none());
    assert_eq!(layout.entries[0].row, layout.results);
}
