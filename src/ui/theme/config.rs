//! Typed theme configuration without filesystem concerns.

use serde::{Deserialize, Deserializer};

/// Explicit or capability-derived terminal theme.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Inherit terminal foreground and background, adapting to capabilities.
    #[default]
    Auto,
    /// Explicit light palette.
    Light,
    /// Explicit dark palette.
    Dark,
    /// Terminal-native limited-color fallback.
    Limited,
}

/// One validated true-color value from user configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThemeColor(pub(crate) u8, pub(crate) u8, pub(crate) u8);

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_hex(&value).map_err(serde::de::Error::custom)
    }
}

/// Optional selected-surface override.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SurfaceColor {
    /// Use a concrete selected surface.
    Color(ThemeColor),
    /// Retain only Proqi's non-color focus cue.
    None,
}

impl<'de> Deserialize<'de> for SurfaceColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.eq_ignore_ascii_case("none") {
            Ok(Self::None)
        } else {
            parse_hex(&value)
                .map(Self::Color)
                .map_err(serde::de::Error::custom)
        }
    }
}

/// Partial semantic color overrides shared by config and theme files.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ThemeOverrides {
    pub(crate) foreground: Option<ThemeColor>,
    pub(crate) background: Option<ThemeColor>,
    pub(crate) accent: Option<ThemeColor>,
    pub(crate) accent_surface: Option<ThemeColor>,
    pub(crate) on_accent: Option<ThemeColor>,
    pub(crate) muted: Option<ThemeColor>,
    pub(crate) divider: Option<ThemeColor>,
    pub(crate) focused_surface: Option<SurfaceColor>,
    pub(crate) link: Option<ThemeColor>,
    pub(crate) annotation: Option<ThemeColor>,
    pub(crate) success: Option<ThemeColor>,
    pub(crate) warning: Option<ThemeColor>,
    pub(crate) error: Option<ThemeColor>,
}

impl ThemeOverrides {
    pub(crate) fn overlay(mut self, later: Self) -> Self {
        macro_rules! replace {
            ($field:ident) => {
                if later.$field.is_some() {
                    self.$field = later.$field;
                }
            };
        }
        replace!(foreground);
        replace!(background);
        replace!(accent);
        replace!(accent_surface);
        replace!(on_accent);
        replace!(muted);
        replace!(divider);
        replace!(focused_surface);
        replace!(link);
        replace!(annotation);
        replace!(success);
        replace!(warning);
        replace!(error);
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Fully loaded theme recipe supplied to the terminal palette resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThemeRecipe {
    pub(crate) base: ThemePreference,
    pub(crate) colors: ThemeOverrides,
    pub(crate) custom: bool,
}

impl ThemeRecipe {
    pub(crate) fn built_in(base: ThemePreference, colors: ThemeOverrides) -> Self {
        let custom = !colors.is_empty();
        Self {
            base,
            colors,
            custom,
        }
    }

    pub(crate) fn custom(base: ThemePreference, colors: ThemeOverrides) -> Self {
        Self {
            base,
            colors,
            custom: true,
        }
    }

    pub(crate) const fn needs_palette(&self) -> bool {
        matches!(self.base, ThemePreference::Auto)
    }
}

impl Default for ThemeRecipe {
    fn default() -> Self {
        Self::built_in(ThemePreference::Auto, ThemeOverrides::default())
    }
}

fn parse_hex(value: &str) -> Result<ThemeColor, String> {
    let digits = value
        .strip_prefix('#')
        .ok_or_else(|| format!("theme color '{value}' must use canonical #RRGGBB notation"))?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "theme color '{value}' must use canonical #RRGGBB notation"
        ));
    }
    let component = |range| {
        u8::from_str_radix(&digits[range], 16)
            .map_err(|_| format!("theme color '{value}' is invalid"))
    };
    Ok(ThemeColor(
        component(0..2)?,
        component(2..4)?,
        component(4..6)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{SurfaceColor, ThemeColor, ThemeOverrides};

    #[test]
    fn colors_require_canonical_six_digit_hex() {
        let parsed: ThemeOverrides = toml::from_str("accent = '#70D69B'").expect("color");
        assert_eq!(parsed.accent, Some(ThemeColor(112, 214, 155)));
        assert!(toml::from_str::<ThemeOverrides>("accent = 'green'").is_err());
        assert!(toml::from_str::<ThemeOverrides>("accent = '#fff'").is_err());
    }

    #[test]
    fn focused_surface_can_remove_the_fill() {
        let parsed: ThemeOverrides = toml::from_str("focused_surface = 'none'").expect("surface");
        assert_eq!(parsed.focused_surface, Some(SurfaceColor::None));
    }

    #[test]
    fn later_overrides_replace_only_present_roles() {
        let first: ThemeOverrides =
            toml::from_str("foreground = '#010203'\naccent = '#040506'").expect("first");
        let later: ThemeOverrides = toml::from_str("accent = '#070809'").expect("later");
        let merged = first.overlay(later);
        assert_eq!(merged.foreground, Some(ThemeColor(1, 2, 3)));
        assert_eq!(merged.accent, Some(ThemeColor(7, 8, 9)));
    }
}
