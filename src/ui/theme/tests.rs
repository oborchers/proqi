use proptest::prelude::*;
use ratatui_core::style::{Color, Modifier};

use super::{TerminalPalette, Theme, ThemeOverrides, ThemePreference, ThemeRecipe, contrast};

fn rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(red, green, blue) => Some((red, green, blue)),
        _ => None,
    }
}

#[test]
fn limited_terminals_never_receive_rgb_colors() {
    for preference in [
        ThemePreference::Auto,
        ThemePreference::Light,
        ThemePreference::Dark,
        ThemePreference::Limited,
    ] {
        let theme = Theme::resolve(preference, false);
        assert_eq!(theme.foreground, Color::Reset);
        assert_eq!(theme.accent, Color::Green);
        assert_eq!(theme.warning, Color::Yellow);
    }
}

#[test]
fn selected_surfaces_are_quiet_and_keep_explicit_contrast() {
    let dark = Theme::resolve(ThemePreference::Dark, true);
    assert_eq!(dark.focused_surface, Some(Color::Rgb(39, 40, 48)));

    let light = Theme::resolve(ThemePreference::Light, true);
    assert_eq!(light.focused_surface, Some(Color::Rgb(236, 236, 240)));

    let automatic = Theme::resolve(ThemePreference::Auto, true);
    assert_eq!(automatic.focused_surface, None);
    assert_eq!(automatic.foreground, Color::Reset);
}

#[test]
fn automatic_theme_preserves_terminal_text_and_derives_a_quiet_surface() {
    let theme = Theme::resolve_with_palette(
        ThemePreference::Auto,
        true,
        Some(TerminalPalette {
            foreground: (201, 205, 224),
            background: (24, 25, 34),
            dark: true,
        }),
    );
    assert_eq!(theme.foreground, Color::Rgb(201, 205, 224));
    assert_eq!(theme.background, Color::Rgb(24, 25, 34));
    assert_eq!(theme.focused_surface, Some(Color::Rgb(42, 43, 52)));
    assert_eq!(theme.focused_style().fg, Some(theme.foreground));
    assert!(contrast((201, 205, 224), (42, 43, 52)) >= 4.5);
    assert!(contrast((112, 214, 155), (42, 43, 52)) >= 4.5);
}

#[test]
fn automatic_light_surface_moves_toward_black() {
    let theme = Theme::resolve_with_palette(
        ThemePreference::Auto,
        true,
        Some(TerminalPalette {
            foreground: (30, 31, 34),
            background: (245, 244, 240),
            dark: false,
        }),
    );
    assert_eq!(theme.focused_surface, Some(Color::Rgb(225, 224, 221)));
    assert!(contrast((30, 31, 34), (225, 224, 221)) >= 4.5);
    assert!(contrast((45, 106, 79), (225, 224, 221)) >= 4.5);
}

#[test]
fn limited_and_failed_auto_detection_keep_a_non_color_focus_cue() {
    for preference in [ThemePreference::Auto, ThemePreference::Limited] {
        assert!(
            Theme::resolve(preference, true)
                .focused_style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}

#[test]
fn explicit_theme_text_pairs_meet_aa_contrast() {
    let dark = Theme::resolve(ThemePreference::Dark, true);
    assert!(contrast((232, 228, 223), (15, 13, 10)) >= 4.5);
    assert!(contrast((232, 228, 223), (39, 40, 48)) >= 4.5);
    assert!(contrast((112, 214, 155), (39, 40, 48)) >= 4.5);
    assert!(contrast((112, 214, 155), (15, 13, 10)) >= 4.5);
    assert!(contrast((250, 250, 248), (45, 106, 79)) >= 4.5);
    assert_eq!(dark.warning, Color::Rgb(204, 160, 58));
    assert!(contrast((204, 160, 58), (15, 13, 10)) >= 4.5);
    assert!(contrast((204, 160, 58), (39, 40, 48)) >= 4.5);

    let light = Theme::resolve(ThemePreference::Light, true);
    assert!(contrast((30, 27, 24), (250, 250, 248)) >= 4.5);
    assert!(contrast((30, 27, 24), (236, 236, 240)) >= 4.5);
    assert!(contrast((45, 106, 79), (236, 236, 240)) >= 4.5);
    assert!(contrast((45, 106, 79), (250, 250, 248)) >= 4.5);
    assert!(contrast((250, 250, 248), (45, 106, 79)) >= 4.5);
    assert_eq!(light.warning, Color::Rgb(148, 95, 14));
    assert!(contrast((148, 95, 14), (250, 250, 248)) >= 4.5);
    assert!(contrast((148, 95, 14), (236, 236, 240)) >= 4.5);
    assert_ne!(dark.accent, light.accent);
    assert_ne!(dark.warning, dark.accent);
    assert_ne!(light.warning, light.accent);
}

#[test]
fn automatic_warning_is_brand_derived_and_contrasts_with_both_surfaces() {
    for palette in [
        TerminalPalette {
            foreground: (201, 205, 224),
            background: (24, 25, 34),
            dark: true,
        },
        TerminalPalette {
            foreground: (30, 31, 34),
            background: (245, 244, 240),
            dark: false,
        },
    ] {
        let theme = Theme::resolve_with_palette(ThemePreference::Auto, true, Some(palette));
        let warning = rgb(theme.warning).expect("true-color warning");
        assert!(contrast(warning, palette.background) >= 4.5);
        if let Some(surface) = theme.focused_surface.and_then(rgb) {
            assert!(contrast(warning, surface) >= 4.5);
        }
        assert_ne!(theme.warning, theme.accent);
    }
}

#[test]
fn inaccessible_custom_theme_is_rejected_before_rendering() {
    let colors: ThemeOverrides =
        toml::from_str("foreground = '#101010'\nbackground = '#111111'").expect("overrides");
    let recipe = ThemeRecipe::custom(ThemePreference::Dark, colors);
    let error = Theme::resolve_recipe(&recipe, true, None).expect_err("contrast failure");
    assert!(error.to_string().contains("foreground on background"));
}

#[test]
fn inline_overrides_are_validated_like_theme_files() {
    let colors: ThemeOverrides = toml::from_str("link = '#111111'").expect("overrides");
    let recipe = ThemeRecipe::built_in(ThemePreference::Dark, colors);
    assert!(Theme::resolve_recipe(&recipe, true, None).is_err());
}

#[test]
fn custom_rgb_theme_degrades_to_limited_colors_when_required() {
    let colors: ThemeOverrides = toml::from_str("link = '#7DD3FC'").expect("overrides");
    let recipe = ThemeRecipe::custom(ThemePreference::Dark, colors);
    let theme = Theme::resolve_recipe(&recipe, false, None).expect("limited fallback");
    assert_eq!(theme.foreground, Color::Reset);
    assert_eq!(theme.link, Color::Green);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_024))]

    #[test]
    fn generated_terminal_palettes_keep_proqi_owned_pairs_accessible(
        candidate_foreground in any::<(u8, u8, u8)>(),
        background in any::<(u8, u8, u8)>(),
        dark in any::<bool>(),
    ) {
        let foreground = if contrast(candidate_foreground, background) >= 4.5 {
            candidate_foreground
        } else {
            super::accessible_monochrome(background)
        };
        let theme = Theme::resolve_with_palette(
            ThemePreference::Auto,
            true,
            Some(TerminalPalette { foreground, background, dark }),
        );
        let accent = rgb(theme.accent).expect("automatic accent is true color");
        let on_accent = rgb(theme.on_accent).expect("automatic accent text is true color");

        prop_assert_eq!(theme.foreground, Color::Rgb(
            foreground.0,
            foreground.1,
            foreground.2,
        ));
        prop_assert!(contrast(accent, background) >= 4.5);
        prop_assert!(contrast(on_accent, accent) >= 4.5);
        let warning = rgb(theme.warning).expect("automatic warning is true color");
        prop_assert!(contrast(warning, background) >= 4.5);
        if let Some(surface) = theme.focused_surface.and_then(rgb) {
            prop_assert!(contrast(foreground, surface) >= 4.5);
            prop_assert!(contrast(accent, surface) >= 4.5);
            prop_assert!(contrast(warning, surface) >= 4.5);
        } else {
            prop_assert!(
                theme
                    .focused_style()
                    .add_modifier
                    .contains(Modifier::REVERSED)
            );
        }
    }
}
