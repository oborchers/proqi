//! Lossless terminal-independent keyboard event vocabulary.

/// Terminal-independent identity of a keyboard event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeyStroke {
    /// Character or named logical key reported by the terminal protocol.
    pub key: LogicalKey,
    /// Individually preserved logical modifiers.
    pub modifiers: LogicalModifiers,
    /// Press, repeat, or release phase.
    pub phase: KeyPhase,
    /// Enhanced keyboard state that can affect key identity.
    pub state: LogicalKeyState,
}

impl KeyStroke {
    /// Construct one ordinary key press without modifiers.
    #[must_use]
    pub const fn press(key: LogicalKey) -> Self {
        Self {
            key,
            modifiers: LogicalModifiers::NONE,
            phase: KeyPhase::Press,
            state: LogicalKeyState::NONE,
        }
    }

    /// Attach an exact logical modifier set.
    #[must_use]
    pub const fn with_modifiers(mut self, modifiers: LogicalModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

/// Logical key independent of Crossterm and terminal escape encodings.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(
    missing_docs,
    reason = "variants mirror the closed terminal key vocabulary"
)]
pub enum LogicalKey {
    Character(char),
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    Function(u8),
    Null,
    Escape,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    Menu,
    KeypadBegin,
    Media(LogicalMediaKey),
    Modifier(LogicalModifierKey),
}

/// Media keys preserved from enhanced keyboard protocols.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(
    missing_docs,
    reason = "variants mirror Crossterm's closed media-key vocabulary"
)]
pub enum LogicalMediaKey {
    Play,
    Pause,
    PlayPause,
    Reverse,
    Stop,
    FastForward,
    Rewind,
    TrackNext,
    TrackPrevious,
    Record,
    LowerVolume,
    RaiseVolume,
    MuteVolume,
}

/// Physical modifier-key identity when the protocol reports it as a key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(
    missing_docs,
    reason = "variants mirror Crossterm's closed modifier-key vocabulary"
)]
pub enum LogicalModifierKey {
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    LeftHyper,
    LeftMeta,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    RightHyper,
    RightMeta,
    IsoLevel3Shift,
    IsoLevel5Shift,
}

/// Keyboard event phase, retained when enhanced reporting distinguishes it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(missing_docs, reason = "the three phase names are self-describing")]
pub enum KeyPhase {
    Press,
    Repeat,
    Release,
}

/// Exact logical modifiers. `Primary` is deliberately not stored here.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogicalModifiers(u8);

impl LogicalModifiers {
    /// No logical modifiers.
    pub const NONE: Self = Self(0);
    /// Shift modifier.
    pub const SHIFT: Self = Self(1 << 0);
    /// Raw Control modifier.
    pub const CONTROL: Self = Self(1 << 1);
    /// Alt or Option modifier.
    pub const ALT: Self = Self(1 << 2);
    /// Super modifier.
    pub const SUPER: Self = Self(1 << 3);
    /// Meta modifier.
    pub const META: Self = Self(1 << 4);
    /// Hyper modifier retained for lossless decoding.
    pub const HYPER: Self = Self(1 << 5);

    /// Combine modifier sets without losing individual identities.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Whether every modifier in `other` is present.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    /// Whether either set contains a shared modifier.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    /// Remove every modifier present in `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
    /// Whether no modifiers are present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Enhanced logical state reported alongside a key.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogicalKeyState(u8);

impl LogicalKeyState {
    /// No enhanced-keyboard state.
    pub const NONE: Self = Self(0);
    /// Key originated from the numeric keypad.
    pub const KEYPAD: Self = Self(1 << 0);
    /// Caps Lock was active.
    pub const CAPS_LOCK: Self = Self(1 << 1);
    /// Num Lock was active.
    pub const NUM_LOCK: Self = Self(1 << 2);
    /// Combine enhanced-keyboard state flags.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}
