use super::{PickerRow, overlay_clear_area, picker_line, picker_row};
use crate::ui::{Theme, ThemePreference};
use ratatui_core::{layout::Rect, style::Modifier};

#[test]
fn two_field_row_right_aligns_qualifier() {
    assert_eq!(
        picker_row(PickerRow::fields("$skill", "Global Skill"), 24),
        "$skill      Global Skill"
    );
}

#[test]
fn narrow_row_hides_qualifier_before_token() {
    assert_eq!(
        picker_row(PickerRow::fields("$long-skill", "Global Skill"), 11),
        "$long-skill"
    );
}

#[test]
fn responsive_row_keeps_location_fallbacks_in_priority_order() {
    let fallbacks = vec!["Workspace · p1".to_owned(), "p1".to_owned()];
    let row = PickerRow::grouped(
        "reviewer",
        "Workspace / tab · p1 · codex · idle",
        &fallbacks,
        Some("Live in Herdr"),
    );

    assert_eq!(picker_row(row, 24), "reviewer  Workspace · p1");
    assert_eq!(picker_row(row, 12), "reviewer  p1");
}

#[test]
fn every_picker_secondary_uses_the_same_quiet_metadata_style() {
    let theme = Theme::resolve(ThemePreference::Dark, true);
    let row = PickerRow::fields("$skill", "Project Skill");

    let ordinary = picker_line(row, 24, false, &theme);
    assert_eq!(ordinary.spans.len(), 3);
    assert_eq!(ordinary.spans[0].style.fg, Some(theme.foreground));
    assert_eq!(ordinary.spans[2].style.fg, Some(theme.muted));

    let selected = picker_line(row, 24, true, &theme);
    assert_eq!(selected.spans.len(), 3);
    assert_eq!(selected.spans[0].style.fg, Some(theme.accent));
    assert!(
        selected.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(selected.spans[2].style.fg, Some(theme.muted));
    assert_eq!(selected.spans[2].style.bg, theme.focused_surface);
    insta::with_settings!({ snapshot_path => "../snapshots" }, {
        insta::assert_debug_snapshot!("picker_metadata_styles", (ordinary, selected));
    });
}

#[test]
fn selected_disabled_choice_keeps_focus_surface_and_muted_text() {
    let theme = Theme::resolve(ThemePreference::Dark, true);
    let selected = picker_line(
        PickerRow::choice("Blocked receiver", "blocked", false),
        32,
        true,
        &theme,
    );

    assert!(
        selected
            .spans
            .iter()
            .all(|span| span.style.bg == theme.focused_surface)
    );
    assert_eq!(selected.spans[0].style.fg, Some(theme.muted));
}

#[test]
fn overlong_token_ellipsizes_on_grapheme_and_cell_boundaries() {
    let rendered = picker_row(PickerRow::fields("$界界e\u{301}🙂", "Global Skill"), 5);

    assert_eq!(rendered, "$界…");
    assert!(crate::ports::text_layout::terminal_cell_width(&rendered) <= 5);
}

#[test]
fn picker_alignment_counts_combining_cjk_and_emoji_cells() {
    let rendered = picker_row(PickerRow::fields("$界e\u{301}", "全👩‍💻"), 16);

    assert!(rendered.ends_with("全👩‍💻"));
    assert_eq!(
        crate::ports::text_layout::terminal_cell_width(&rendered),
        16
    );
}

#[test]
fn picker_alignment_sanitizes_tabs_and_controls_in_both_fields() {
    let rendered = picker_row(PickerRow::fields("a\tb\u{7}", "x\ty\u{7}"), 18);

    assert_eq!(
        crate::ports::text_layout::terminal_cell_width(&rendered),
        18
    );
    assert!(!rendered.contains(['\t', '\u{7}']));
    assert!(rendered.starts_with("a   b�"));
    assert!(rendered.ends_with("x   y�"));
}

#[test]
fn overlay_clear_halo_clamps_to_the_viewport() {
    let viewport = Rect::new(4, 2, 20, 8);
    assert_eq!(
        overlay_clear_area(viewport, Rect::new(8, 3, 10, 4)),
        Rect::new(7, 3, 12, 4)
    );
    assert_eq!(
        overlay_clear_area(viewport, Rect::new(4, 2, 20, 8)),
        viewport
    );
}
