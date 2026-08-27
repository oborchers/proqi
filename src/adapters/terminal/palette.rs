//! Bounded terminal palette discovery before the input lane starts.

use std::time::Duration;

use terminal_colorsaurus::{QueryOptions, ThemeMode, color_palette};

use crate::ui::{TerminalPalette, Theme, ThemeRecipe};

use super::TerminalError;

/// Resolve the UI theme without allowing a palette probe to consume live input.
pub(super) fn resolve(recipe: &ThemeRecipe, true_color: bool) -> Result<Theme, TerminalError> {
    if !true_color || !recipe.needs_palette() {
        return Theme::resolve_recipe(recipe, true_color, None)
            .map_err(|error| TerminalError::Config(error.to_string()));
    }
    let mut options = QueryOptions::default();
    options.timeout = Duration::from_millis(250);
    let palette = color_palette(options).ok().map(|palette| TerminalPalette {
        foreground: palette.foreground.scale_to_8bit(),
        background: palette.background.scale_to_8bit(),
        dark: palette.theme_mode() == ThemeMode::Dark,
    });
    Theme::resolve_recipe(recipe, true_color, palette)
        .map_err(|error| TerminalError::Config(error.to_string()))
}
