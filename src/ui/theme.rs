//! Small capability-aware terminal palette.

use ratatui_core::style::{Color, Modifier, Style};

mod config;
mod validation;

pub use config::ThemePreference;
pub(crate) use config::{SurfaceColor, ThemeColor, ThemeOverrides, ThemeRecipe};
pub(crate) use validation::ThemeError;

/// Terminal colors discovered before the full-screen interface starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalPalette {
    /// Default terminal foreground.
    pub foreground: (u8, u8, u8),
    /// Default terminal background.
    pub background: (u8, u8, u8),
    /// Whether the terminal background is perceptually dark.
    pub dark: bool,
}

/// Resolved colors used by every board widget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    /// Primary terminal text.
    pub foreground: Color,
    /// Terminal background, inherited in automatic and limited modes.
    pub background: Color,
    /// Forest-green routine accent.
    pub accent: Color,
    /// Deeper forest-green surface used by the focus gutter.
    pub accent_surface: Color,
    /// High-contrast text rendered on an accent surface.
    pub on_accent: Color,
    /// Secondary text and quiet controls.
    pub muted: Color,
    /// Quiet horizontal separation between adjacent thoughts.
    pub divider: Color,
    /// Neutral selected-thought surface in explicit themes.
    pub focused_surface: Option<Color>,
    /// Semantic failure color.
    pub error: Color,
    /// Explicit links in thought content.
    pub link: Color,
    /// Folded image, file, and pasted-text annotations.
    pub annotation: Color,
    /// Successful transient status.
    pub success: Color,
    /// Warning transient status.
    pub warning: Color,
}

impl Theme {
    /// Resolve a theme without assuming unsupported terminal colors.
    #[must_use]
    pub fn resolve(preference: ThemePreference, true_color: bool) -> Self {
        Self::resolve_with_palette(preference, true_color, None)
    }

    /// Resolve a theme using the terminal's detected foreground and background.
    #[must_use]
    pub fn resolve_with_palette(
        preference: ThemePreference,
        true_color: bool,
        palette: Option<TerminalPalette>,
    ) -> Self {
        if !true_color || matches!(preference, ThemePreference::Limited) {
            return Self::limited();
        }
        match preference {
            ThemePreference::Light => Self {
                foreground: Color::Rgb(30, 27, 24),
                background: Color::Rgb(250, 250, 248),
                accent: Color::Rgb(45, 106, 79),
                accent_surface: Color::Rgb(45, 106, 79),
                on_accent: Color::Rgb(250, 250, 248),
                muted: Color::Rgb(79, 70, 62),
                divider: Color::Rgb(224, 217, 207),
                focused_surface: Some(Color::Rgb(236, 236, 240)),
                error: Color::Red,
                link: Color::Rgb(45, 106, 79),
                annotation: Color::Rgb(45, 106, 79),
                success: Color::Rgb(45, 106, 79),
                warning: Color::Rgb(45, 106, 79),
            },
            ThemePreference::Dark => Self {
                foreground: Color::Rgb(232, 228, 223),
                background: Color::Rgb(15, 13, 10),
                accent: Color::Rgb(112, 214, 155),
                accent_surface: Color::Rgb(45, 106, 79),
                on_accent: Color::Rgb(250, 250, 248),
                muted: Color::Rgb(176, 169, 160),
                divider: Color::Rgb(42, 37, 32),
                focused_surface: Some(Color::Rgb(39, 40, 48)),
                error: Color::LightRed,
                link: Color::Rgb(112, 214, 155),
                annotation: Color::Rgb(112, 214, 155),
                success: Color::Rgb(112, 214, 155),
                warning: Color::Rgb(112, 214, 155),
            },
            ThemePreference::Auto => match palette {
                Some(palette) => Self::automatic(palette),
                None => Self::limited(),
            },
            ThemePreference::Limited => Self::limited(),
        }
    }

    /// Resolve and validate a fully loaded built-in or custom recipe.
    pub(crate) fn resolve_recipe(
        recipe: &ThemeRecipe,
        true_color: bool,
        palette: Option<TerminalPalette>,
    ) -> Result<Self, ThemeError> {
        if !true_color || matches!(recipe.base, ThemePreference::Limited) {
            return Ok(Self::limited());
        }
        let mut theme = Self::resolve_with_palette(recipe.base, true_color, palette);
        if recipe.custom && matches!(theme.foreground, Color::Reset) {
            return Ok(Self::limited());
        }
        theme.apply(&recipe.colors);
        if recipe.custom {
            validation::validate(&theme)?;
        }
        Ok(theme)
    }

    /// Base foreground and background style.
    #[must_use]
    pub const fn base_style(self) -> Style {
        Style::new().fg(self.foreground).bg(self.background)
    }

    /// High-contrast selected-thought style without turning body text green.
    #[must_use]
    pub fn focused_style(self) -> Style {
        self.focused_surface.map_or_else(
            || self.base_style().add_modifier(Modifier::REVERSED),
            |surface| self.base_style().bg(surface),
        )
    }

    const fn limited() -> Self {
        Self {
            foreground: Color::Reset,
            background: Color::Reset,
            accent: Color::Green,
            accent_surface: Color::Green,
            on_accent: Color::Black,
            muted: Color::DarkGray,
            divider: Color::DarkGray,
            focused_surface: None,
            error: Color::Red,
            link: Color::Green,
            annotation: Color::Green,
            success: Color::Green,
            warning: Color::Green,
        }
    }

    fn automatic(palette: TerminalPalette) -> Self {
        let foreground = Color::Rgb(
            palette.foreground.0,
            palette.foreground.1,
            palette.foreground.2,
        );
        let background = Color::Rgb(
            palette.background.0,
            palette.background.1,
            palette.background.2,
        );
        let target = if palette.dark { 255 } else { 0 };
        let surface = blend(palette.background, target, 8);
        let mut theme = if palette.dark {
            Self::resolve(ThemePreference::Dark, true)
        } else {
            Self::resolve(ThemePreference::Light, true)
        };
        theme.foreground = foreground;
        theme.background = background;
        let accent = accessible_accent(palette.background, palette.dark);
        theme.accent = Color::Rgb(accent.0, accent.1, accent.2);
        theme.accent_surface = theme.accent;
        let on_accent = accessible_monochrome(accent);
        theme.on_accent = Color::Rgb(on_accent.0, on_accent.1, on_accent.2);
        theme.focused_surface = (contrast(palette.foreground, surface) >= 4.5
            && contrast(accent, surface) >= 4.5)
            .then_some(Color::Rgb(surface.0, surface.1, surface.2));
        theme.link = theme.accent;
        theme.annotation = theme.accent;
        theme.success = theme.accent;
        theme.warning = theme.accent;
        theme
    }

    fn apply(&mut self, colors: &ThemeOverrides) {
        macro_rules! apply {
            ($field:ident) => {
                if let Some(color) = colors.$field {
                    self.$field = color.into();
                }
            };
        }
        apply!(foreground);
        apply!(background);
        apply!(accent);
        apply!(accent_surface);
        apply!(on_accent);
        apply!(muted);
        apply!(divider);
        apply!(link);
        apply!(annotation);
        apply!(success);
        apply!(warning);
        apply!(error);
        if let Some(surface) = colors.focused_surface {
            self.focused_surface = match surface {
                SurfaceColor::Color(color) => Some(color.into()),
                SurfaceColor::None => None,
            };
        }
    }
}

impl From<ThemeColor> for Color {
    fn from(value: ThemeColor) -> Self {
        Self::Rgb(value.0, value.1, value.2)
    }
}

fn accessible_accent(background: (u8, u8, u8), dark: bool) -> (u8, u8, u8) {
    let preferred = if dark { (112, 214, 155) } else { (45, 106, 79) };
    let alternate = if dark { (45, 106, 79) } else { (112, 214, 155) };
    [preferred, alternate, accessible_monochrome(background)]
        .into_iter()
        .find(|candidate| contrast(*candidate, background) >= 4.5)
        .unwrap_or_else(|| accessible_monochrome(background))
}

fn accessible_monochrome(color: (u8, u8, u8)) -> (u8, u8, u8) {
    let black = (0, 0, 0);
    let white = (255, 255, 255);
    if contrast(black, color) >= contrast(white, color) {
        black
    } else {
        white
    }
}

fn blend(color: (u8, u8, u8), target: u8, percentage: u16) -> (u8, u8, u8) {
    (
        blend_channel(color.0, target, percentage),
        blend_channel(color.1, target, percentage),
        blend_channel(color.2, target, percentage),
    )
}

fn blend_channel(channel: u8, target: u8, percentage: u16) -> u8 {
    let retained = 100_u16.saturating_sub(percentage);
    let value = u16::from(channel) * retained + u16::from(target) * percentage + 50;
    u8::try_from(value / 100).unwrap_or(u8::MAX)
}

use validation::contrast;

#[cfg(test)]
mod tests {
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

        let light = Theme::resolve(ThemePreference::Light, true);
        assert!(contrast((30, 27, 24), (250, 250, 248)) >= 4.5);
        assert!(contrast((30, 27, 24), (236, 236, 240)) >= 4.5);
        assert!(contrast((45, 106, 79), (236, 236, 240)) >= 4.5);
        assert!(contrast((45, 106, 79), (250, 250, 248)) >= 4.5);
        assert!(contrast((250, 250, 248), (45, 106, 79)) >= 4.5);
        assert_ne!(dark.accent, light.accent);
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
            if let Some(surface) = theme.focused_surface.and_then(rgb) {
                prop_assert!(contrast(foreground, surface) >= 4.5);
                prop_assert!(contrast(accent, surface) >= 4.5);
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
}
