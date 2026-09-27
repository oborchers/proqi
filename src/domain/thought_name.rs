//! Optional organizational names for thoughts.

use serde::{Deserialize, Deserializer, Serialize, de};

use super::DomainError;

/// Maximum number of Unicode scalar values in a thought name.
pub const THOUGHT_NAME_MAX_CHARS: usize = 80;

/// A short, single-line organizational label which is not thought content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ThoughtName(String);

impl<'de> Deserialize<'de> for ThoughtName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

impl ThoughtName {
    /// Validate and construct a non-empty thought name.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidThoughtName`] for blank, overlong, or
    /// control-bearing input.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.trim() != value
            || value.chars().count() > THOUGHT_NAME_MAX_CHARS
            || value.chars().any(is_disallowed)
        {
            return Err(DomainError::InvalidThoughtName);
        }
        Ok(Self(value))
    }

    /// Borrow the exact validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_disallowed(character: char) -> bool {
    character.is_control() || matches!(character, '\u{0085}' | '\u{2028}' | '\u{2029}')
}

#[cfg(test)]
mod tests {
    use super::{THOUGHT_NAME_MAX_CHARS, ThoughtName};

    #[test]
    fn accepts_short_unicode_and_preserves_internal_whitespace() {
        let name = ThoughtName::new("設計  note").expect("valid name");
        assert_eq!(name.as_str(), "設計  note");
    }

    #[test]
    fn rejects_blank_edges_controls_and_overlong_values() {
        for value in ["", " ", " leading", "trailing ", "line\nfeed", "a\u{0085}b"] {
            assert!(ThoughtName::new(value).is_err(), "accepted {value:?}");
        }
        assert!(ThoughtName::new("x".repeat(THOUGHT_NAME_MAX_CHARS)).is_ok());
        assert!(ThoughtName::new("x".repeat(THOUGHT_NAME_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn deserialization_enforces_the_value_object_invariant() {
        assert!(serde_json::from_str::<ThoughtName>(r#""valid""#).is_ok());
        assert!(serde_json::from_str::<ThoughtName>(r#"" line\nbreak""#).is_err());
    }
}
