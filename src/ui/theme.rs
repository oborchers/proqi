//! Small capability-aware terminal palette.

use ratatui_core::style::{Color, Style};

use super::ThemePreference;

/// Resolved colors used by every board widget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    /// Primary terminal text.
    pub foreground: Color,
    /// Terminal background, inherited in automatic and limited modes.
    pub background: Color,
    /// Forest-green routine accent.
    pub accent: Color,
    /// Deeper forest-green surface used by the gutter and text cursor.
    pub accent_surface: Color,
    /// High-contrast text rendered on an accent surface.
    pub on_accent: Color,
    /// Secondary text and quiet controls.
    pub muted: Color,
    /// Quiet horizontal separation between adjacent thoughts.
    pub divider: Color,
    /// Neutral selected-thought surface in explicit themes.
    pub focused_surface: Option<Color>,
    /// Text color used on the selected-thought surface.
    pub focused_foreground: Color,
    /// Semantic failure color.
    pub error: Color,
}

impl Theme {
    /// Resolve a theme without assuming unsupported terminal colors.
    #[must_use]
    pub const fn resolve(preference: ThemePreference, true_color: bool) -> Self {
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
                focused_foreground: Color::Rgb(30, 27, 24),
                error: Color::Red,
            },
            ThemePreference::Dark => Self {
                foreground: Color::Rgb(232, 228, 223),
                background: Color::Rgb(15, 13, 10),
                accent: Color::Rgb(112, 214, 155),
                accent_surface: Color::Rgb(45, 106, 79),
                on_accent: Color::Rgb(250, 250, 248),
                muted: Color::Rgb(176, 169, 160),
                divider: Color::Rgb(42, 37, 32),
                focused_surface: Some(Color::Rgb(52, 52, 63)),
                focused_foreground: Color::Rgb(232, 228, 223),
                error: Color::LightRed,
            },
            ThemePreference::Auto => Self {
                foreground: Color::Reset,
                background: Color::Reset,
                accent: Color::Green,
                accent_surface: Color::Green,
                on_accent: Color::Black,
                muted: Color::DarkGray,
                divider: Color::DarkGray,
                focused_surface: Some(Color::DarkGray),
                focused_foreground: Color::White,
                error: Color::Red,
            },
            ThemePreference::Limited => Self::limited(),
        }
    }

    /// Base foreground and background style.
    #[must_use]
    pub const fn base_style(self) -> Style {
        Style::new().fg(self.foreground).bg(self.background)
    }

    /// High-contrast selected-thought style without turning body text green.
    #[must_use]
    pub fn focused_style(self) -> Style {
        self.base_style()
            .fg(self.focused_foreground)
            .bg(self.focused_surface.unwrap_or(Color::DarkGray))
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
            focused_foreground: Color::White,
            error: Color::Red,
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui_core::style::Color;

    use super::{Theme, ThemePreference};

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
        assert_eq!(dark.focused_surface, Some(Color::Rgb(52, 52, 63)));
        assert_eq!(dark.focused_foreground, Color::Rgb(232, 228, 223));

        let light = Theme::resolve(ThemePreference::Light, true);
        assert_eq!(light.focused_surface, Some(Color::Rgb(236, 236, 240)));
        assert_eq!(light.focused_foreground, Color::Rgb(30, 27, 24));

        let automatic = Theme::resolve(ThemePreference::Auto, true);
        assert_eq!(automatic.focused_surface, Some(Color::DarkGray));
        assert_eq!(automatic.focused_foreground, Color::White);
    }

    #[test]
    fn explicit_accent_text_meets_aa_contrast() {
        let dark = Theme::resolve(ThemePreference::Dark, true);
        assert!(contrast(dark.accent, Color::Rgb(52, 52, 63)) >= 4.5);
        assert!(contrast(dark.accent, dark.background) >= 4.5);

        let light = Theme::resolve(ThemePreference::Light, true);
        assert!(contrast(light.accent, light.focused_surface.expect("surface")) >= 4.5);
        assert!(contrast(light.accent, light.background) >= 4.5);
    }

    fn contrast(first: Color, second: Color) -> f64 {
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    fn luminance(color: Color) -> f64 {
        let Color::Rgb(red, green, blue) = color else {
            return 0.0;
        };
        [red, green, blue]
            .map(|channel| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.040_45 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            })
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(channel, weight)| channel * weight)
            .sum()
    }
}
