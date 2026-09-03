//! Canonical platform spelling for user-facing Primary shortcut labels.

/// Platform projection used only to render the logical Primary modifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrimaryKeyPlatform {
    MacOs,
    Portable,
}

impl PrimaryKeyPlatform {
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Portable
        }
    }

    pub(crate) fn label(self, suffix: &str) -> String {
        let prefix = match self {
            Self::MacOs => "Cmd",
            Self::Portable => "Ctrl",
        };
        format!("{prefix}+{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_labels_are_platform_specific() {
        assert_eq!(PrimaryKeyPlatform::MacOs.label("C"), "Cmd+C");
        assert_eq!(PrimaryKeyPlatform::Portable.label("C"), "Ctrl+C");
    }
}
