//! Completion choices scroll: keyboard, wheel, and rendered rows reach the same choices.

use ratatui_core::layout::Rect;

use super::Fixture;
use super::rendering::{click, snapshot};
use crate::{
    application::Effect,
    domain::ExportDisposition,
    ports::export::{DirectoryEntry, DirectoryListing},
    ui::{HitTarget, PointerInput, PointerKind, UiInput, UiKey},
};

fn listing(names: &[String], truncated: bool) -> DirectoryListing {
    DirectoryListing {
        entries: names
            .iter()
            .map(|name| DirectoryEntry {
                name: name.clone(),
                directory: false,
            })
            .collect(),
        truncated,
    }
}

fn offered(names: &[String], truncated: bool) -> Fixture {
    let mut fixture = Fixture::new(&[("body", None)]);
    fixture.app.state.focused_item = fixture.app.live_item_ids().first().copied();
    fixture.begin(ExportDisposition::Keep);
    fixture.set_field("n");
    let effects = fixture.key(UiKey::Tab);
    let [Effect::ListExportDirectory { generation, .. }] = effects.as_slice() else {
        panic!("listing");
    };
    let generation = *generation;
    fixture
        .app
        .complete_export_listing(generation, Ok(listing(names, truncated)));
    fixture
}

fn names(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("n{index:02}.txt")).collect()
}

fn visible_rows(fixture: &mut Fixture) -> (Vec<String>, usize, (bool, bool)) {
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 60, 10));
    let visible = layout.overlay.as_ref().expect("overlay").items.len();
    let Some(super::super::ExportView::Path {
        entries, selected, ..
    }) = fixture.app.export_view()
    else {
        panic!("path stage");
    };
    (
        entries.into_iter().take(visible).collect(),
        selected,
        fixture.app.picker_overflow(visible),
    )
}

fn wheel(fixture: &mut Fixture, kind: PointerKind) {
    fixture.app.handle(
        UiInput::Pointer(PointerInput {
            column: 10,
            row: 5,
            kind,
            extend_selection: false,
        }),
        &mut fixture.ids,
        &fixture.clock,
    );
}

#[test]
fn cycling_scrolls_the_choices_and_keeps_the_highlight_visible() {
    let mut fixture = offered(&names(12), false);
    let (rows, selected, overflow) = visible_rows(&mut fixture);
    assert_eq!(rows[selected], "n00.txt", "the first choice is cycled in");
    assert_eq!(overflow, (false, true), "more choices below: {rows:?}");
    for _ in 0..11 {
        fixture.key(UiKey::Tab);
    }
    assert_eq!(fixture.field(), "n11.txt");
    let (rows, selected, overflow) = visible_rows(&mut fixture);
    assert_eq!(rows[selected], "n11.txt", "{rows:?}");
    assert_eq!(overflow, (true, false));
    insta::with_settings!({snapshot_path => "../../../snapshots"}, {
        insta::assert_snapshot!("export_completion_scrolled", snapshot(&mut fixture, 60, 10));
    });
}

#[test]
fn wheel_and_rendered_rows_reach_the_same_choices_as_the_keyboard() {
    let mut keyboard = offered(&names(12), false);
    let mut pointer = offered(&names(12), false);
    for _ in 0..9 {
        keyboard.key(UiKey::Tab);
        wheel(&mut pointer, PointerKind::ScrollDown);
    }
    assert_eq!(keyboard.field(), pointer.field());
    assert_eq!(visible_rows(&mut keyboard), visible_rows(&mut pointer));
    keyboard.key(UiKey::BackTab);
    wheel(&mut pointer, PointerKind::ScrollUp);
    assert_eq!(keyboard.field(), "n08.txt");
    assert_eq!(pointer.field(), "n08.txt");

    // A rendered row activates the choice it shows, not the unscrolled one.
    let (rows, _, _) = visible_rows(&mut pointer);
    let row = rows.len() - 1;
    let shown = rows[row].clone();
    assert!(click(&mut pointer, HitTarget::PaletteItem(row)).is_empty());
    assert_eq!(pointer.field(), shown);
}

#[test]
fn a_truncated_listing_asks_for_more_of_the_name() {
    let fixture = offered(&["n-only.txt".to_owned()], true);
    assert_eq!(fixture.field(), "n", "no unique completion is claimed");
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("too large to complete"))
    );
    let mut fixture = offered(&[], true);
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| !status.contains("no matching"))
    );
    fixture.key(UiKey::Character('x'));
    assert_eq!(fixture.field(), "nx");
}

#[test]
fn more_matches_than_offered_rows_are_announced() {
    let fixture = offered(&names(80), false);
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("showing 64 of 80 matches")),
        "{:?}",
        fixture.app.status_text()
    );
}
