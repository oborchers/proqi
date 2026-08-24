//! Bounded terminal palette discovery before the input lane starts.

use std::time::Duration;

use terminal_colorsaurus::{QueryOptions, ThemeMode, color_palette};

use crate::ui::{TerminalPalette, Theme, ThemePreference};

/// Resolve the UI theme without allowing a palette probe to consume live input.
pub(super) fn resolve(preference: ThemePreference, true_color: bool) -> Theme {
    if !true_color || !matches!(preference, ThemePreference::Auto) {
        return Theme::resolve(preference, true_color);
    }
    let mut options = QueryOptions::default();
    options.timeout = Duration::from_millis(250);
    let palette = color_palette(options).ok().map(|palette| TerminalPalette {
        foreground: palette.foreground.scale_to_8bit(),
        background: palette.background.scale_to_8bit(),
        dark: palette.theme_mode() == ThemeMode::Dark,
    });
    Theme::resolve_with_palette(preference, true_color, palette)
}
