//! Platform-qualified snapshot assertion surface.

macro_rules! assert_platform_snapshot {
    ($value:expr) => {{
        let platform = $crate::platform_suffix();
        insta::with_settings!({ snapshot_suffix => platform }, {
            insta::assert_snapshot!($value);
        });
    }};
    ($name:expr, $value:expr) => {{
        let platform = $crate::platform_suffix();
        insta::with_settings!({ snapshot_suffix => platform }, {
            insta::assert_snapshot!($name, $value);
        });
    }};
}

pub(super) use assert_platform_snapshot;
