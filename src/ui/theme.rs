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
                muted: Color::Rgb(79, 70, 62),
                divider: Color::Rgb(224, 217, 207),
                focused_surface: Some(Color::Rgb(236, 236, 240)),
                focused_foreground: Color::Rgb(30, 27, 24),
                error: Color::Red,
            },
            ThemePreference::Dark => Self {
                foreground: Color::Rgb(232, 228, 223),
                background: Color::Rgb(15, 13, 10),
                accent: Color::Rgb(91, 158, 125),
                muted: Color::Rgb(176, 169, 160),
                divider: Color::Rgb(42, 37, 32),
                focused_surface: Some(Color::Rgb(52, 52, 63)),
                focused_foreground: Color::Rgb(232, 228, 223),
                error: Color::LightRed,
            },
            ThemePreference::Auto => Self {
                foreground: Color::Reset,
                background: Color::Reset,
                accent: Color::Rgb(45, 106, 79),
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
}
