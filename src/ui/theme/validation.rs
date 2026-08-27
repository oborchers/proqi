//! Accessibility validation for fully resolved custom themes.

use ratatui_core::style::Color;
use thiserror::Error;

use super::Theme;

/// Invalid custom theme that Proqi refuses to render.
#[derive(Clone, Debug, Error, PartialEq)]
pub(crate) enum ThemeError {
    /// A measurable semantic pair does not meet its required contrast.
    #[error(
        "theme contrast for {foreground_role} on {background_role} is {actual:.2}:1; at least {required:.1}:1 is required"
    )]
    Contrast {
        foreground_role: &'static str,
        background_role: &'static str,
        actual: f64,
        required: f64,
    },
}

pub(super) fn validate(theme: &Theme) -> Result<(), ThemeError> {
    let background = rgb(theme.background);
    for (role, color) in [
        ("foreground", theme.foreground),
        ("muted", theme.muted),
        ("accent", theme.accent),
        ("link", theme.link),
        ("annotation", theme.annotation),
        ("success", theme.success),
        ("warning", theme.warning),
        ("error", theme.error),
    ] {
        check(role, color, "background", background, 4.5)?;
    }
    check(
        "on_accent",
        theme.on_accent,
        "accent_surface",
        rgb(theme.accent_surface),
        4.5,
    )?;
    check(
        "accent_surface",
        theme.accent_surface,
        "background",
        background,
        3.0,
    )?;
    if let Some(surface) = theme.focused_surface {
        for (role, color) in [
            ("foreground", theme.foreground),
            ("accent", theme.accent),
            ("link", theme.link),
            ("annotation", theme.annotation),
        ] {
            check(role, color, "focused_surface", rgb(surface), 4.5)?;
        }
    }
    Ok(())
}

fn check(
    foreground_role: &'static str,
    foreground: Color,
    background_role: &'static str,
    background: Option<(u8, u8, u8)>,
    required: f64,
) -> Result<(), ThemeError> {
    let (Some(foreground), Some(background)) = (rgb(foreground), background) else {
        return Ok(());
    };
    let actual = contrast(foreground, background);
    if actual >= required {
        Ok(())
    } else {
        Err(ThemeError::Contrast {
            foreground_role,
            background_role,
            actual,
            required,
        })
    }
}

pub(super) fn contrast(first: (u8, u8, u8), second: (u8, u8, u8)) -> f64 {
    let first = luminance(first);
    let second = luminance(second);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

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

fn rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((128, 0, 0)),
        Color::Green => Some((0, 128, 0)),
        Color::Yellow => Some((128, 128, 0)),
        Color::Blue => Some((0, 0, 128)),
        Color::Magenta => Some((128, 0, 128)),
        Color::Cyan => Some((0, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 0, 0)),
        Color::LightGreen => Some((0, 255, 0)),
        Color::LightYellow => Some((255, 255, 0)),
        Color::LightBlue => Some((0, 0, 255)),
        Color::LightMagenta => Some((255, 0, 255)),
        Color::LightCyan => Some((0, 255, 255)),
        Color::White => Some((255, 255, 255)),
        Color::Rgb(red, green, blue) => Some((red, green, blue)),
        Color::Reset | Color::Indexed(_) => None,
    }
}
