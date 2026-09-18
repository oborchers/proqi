//! Cardinal direction value used at pane boundaries.

use serde::{Deserialize, Serialize};

/// Cardinal direction to an adjacent terminal pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Pane above Proqi.
    Up,
    /// Pane to the right of Proqi.
    Right,
    /// Pane below Proqi.
    Down,
    /// Pane to the left of Proqi.
    Left,
}

impl Direction {
    /// Stable lowercase representation used at external and durable boundaries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Right => "right",
            Self::Down => "down",
            Self::Left => "left",
        }
    }
}
