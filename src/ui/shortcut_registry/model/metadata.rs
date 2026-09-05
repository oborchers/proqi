//! Typed presentation and visibility policy owned by action descriptors.

/// Underlying surface whose contextual Help includes an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpSurface {
    Board,
    Editor,
}

/// Capability that controls whether a Help item is currently visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpAvailability {
    Always,
    Submission,
    EffectiveTransform,
}

/// One ordered Help projection attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpMetadata {
    pub(crate) surface: HelpSurface,
    pub(crate) order: u8,
    pub(crate) label: &'static str,
    pub(crate) availability: HelpAvailability,
}

/// Footer copy and measurement policy attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FooterMetadata {
    pub(crate) text: &'static str,
    pub(crate) compact_text: &'static str,
    pub(crate) minimum_width: u16,
    pub(crate) compact_minimum_width: u16,
}

/// Runtime capability required for one Commands entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandAvailability {
    Always,
    Submission,
    Editor,
    ScreenshotRetry,
    Split,
    Extract,
    Merge,
    ScreenshotInbox,
}

/// Stable or state-dependent Commands label policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandLabel {
    Static(&'static str),
    ScreenshotInbox {
        enable: &'static str,
        disable: &'static str,
        resume: &'static str,
        unavailable: &'static str,
    },
}

/// Ordered Commands projection attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommandMetadata {
    pub(crate) order: u8,
    pub(crate) label: CommandLabel,
    pub(crate) availability: CommandAvailability,
}
