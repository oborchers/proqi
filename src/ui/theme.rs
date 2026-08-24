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
                focused_surface: Some(Color::Rgb(39, 40, 48)),
                error: Color::LightRed,
            },
            ThemePreference::Auto | ThemePreference::Limited => Self::limited(),
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
        self.focused_surface.map_or_else(
            || self.base_style(),
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
        }
    }
}

#[cfg(test)]
fn contrast(first: (u8, u8, u8), second: (u8, u8, u8)) -> f64 {
    let first = luminance(first);
    let second = luminance(second);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

#[cfg(test)]
fn luminance(color: (u8, u8, u8)) -> f64 {
    [color.0, color.1, color.2]
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

#[cfg(test)]
mod tests {
    use ratatui_core::style::Color;

    use super::{Theme, ThemePreference, contrast};

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
}
