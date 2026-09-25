//! Export overlay rendering, responsive snapshots, and pointer rows from the rendered frame.

use ratatui_core::{backend::TestBackend, layout::Rect, terminal::Terminal};

use super::{EXISTING, Fixture};
use crate::{
    application::Effect,
    domain::ExportDisposition,
    ports::environment::Clock as _,
    ports::export::{DirectoryEntry, DirectoryListing, ExportWriteError},
    ui::{
        HitTarget, PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput,
        UiKey, render,
    },
};

fn snapshot(fixture: &mut Fixture, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = fixture.app.prepare_frame(frame.area());
            render(
                frame,
                &fixture.app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn path_stage() -> Fixture {
    let mut fixture = Fixture::new(&[("Grüße 👩‍💻 first", None), ("second", None)]);
    fixture.select_all();
    fixture.begin(ExportDisposition::ReplaceWithReference);
    fixture.set_field("notes/Grüße.txt");
    fixture
}

fn completion_stage() -> Fixture {
    let mut fixture = path_stage();
    fixture.set_field("d");
    let effects = fixture.key(UiKey::Tab);
    let [Effect::ListExportDirectory { generation, .. }] = effects.as_slice() else {
        panic!("listing");
    };
    let generation = *generation;
    fixture.app.complete_export_listing(
        generation,
        Ok(DirectoryListing {
            entries: [("docs", true), ("drafts", true), ("design.txt", false)]
                .iter()
                .map(|(name, directory)| DirectoryEntry {
                    name: (*name).to_owned(),
                    directory: *directory,
                })
                .collect(),
            truncated: false,
        }),
    );
    fixture
}

fn confirm_stage() -> Fixture {
    let mut fixture = path_stage();
    let (request_id, _) = fixture.submit();
    fixture.complete(request_id, Err(ExportWriteError::Exists(EXISTING)));
    fixture
}

#[test]
fn destination_field_has_standard_narrow_and_shallow_snapshots() {
    insta::with_settings!({snapshot_path => "../../../snapshots"}, {
        insta::assert_snapshot!("export_path_standard", snapshot(&mut path_stage(), 72, 14));
        insta::assert_snapshot!("export_path_narrow", snapshot(&mut path_stage(), 34, 12));
        insta::assert_snapshot!("export_path_shallow", snapshot(&mut path_stage(), 60, 6));
        insta::assert_snapshot!("export_completion_standard", snapshot(&mut completion_stage(), 72, 14));
        insta::assert_snapshot!("export_replace_confirmation", snapshot(&mut confirm_stage(), 72, 12));
        insta::assert_snapshot!("export_replace_confirmation_narrow", snapshot(&mut confirm_stage(), 34, 10));
    });
}

/// Click one rendered overlay row, later than the multi-click window of the previous click.
fn click(fixture: &mut Fixture, target: HitTarget) -> Vec<Effect> {
    let later = fixture.clock.now().as_millis() + 1_000;
    fixture
        .clock
        .set(crate::domain::Timestamp::from_millis(later));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 14));
    let overlay = layout.overlay.as_ref().expect("overlay");
    let area = match target {
        HitTarget::PaletteItem(index) => overlay.items[index],
        HitTarget::CloseOverlay => overlay.close,
        _ => panic!("overlay target"),
    };
    assert_eq!(layout.hit_test(area.x, area.y), Some(target));
    fixture.app.handle(
        UiInput::Pointer(PointerInput {
            column: area.x,
            row: area.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut fixture.ids,
        &fixture.clock,
    )
}

#[test]
fn rendered_rows_are_the_pointer_targets_for_completion_saving_and_confirmation() {
    let mut fixture = completion_stage();
    assert!(click(&mut fixture, HitTarget::PaletteItem(2)).is_empty());
    assert_eq!(fixture.field(), "drafts/");
    fixture.set_field("drafts/out.txt");
    let effects = click(&mut fixture, HitTarget::PaletteItem(0));
    let [Effect::WriteExport { request_id, .. }] = effects.as_slice() else {
        panic!("save by pointer: {effects:?}");
    };
    let request_id = *request_id;
    fixture.complete(request_id, Err(ExportWriteError::Exists(EXISTING)));
    assert!(click(&mut fixture, HitTarget::PaletteItem(0)).is_empty());
    assert_eq!(
        fixture.field(),
        "drafts/out.txt",
        "pointer Cancel keeps the path"
    );
    let (request_id, _) = fixture.submit();
    fixture.complete(request_id, Err(ExportWriteError::Exists(EXISTING)));
    let effects = click(&mut fixture, HitTarget::PaletteItem(1));
    assert!(matches!(effects.as_slice(), [Effect::WriteExport { .. }]));

    let mut fixture = path_stage();
    assert!(click(&mut fixture, HitTarget::CloseOverlay).is_empty());
    assert!(fixture.app.export.active.is_none());
    assert_eq!(fixture.contents().len(), 2);
}
