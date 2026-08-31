//! Integration-test crate registry for real process and pseudo-terminal behavior.
//!
//! Shared harness primitives live in `pty/support.rs`. Every product scenario
//! lives in one behavior-owned sibling module.

#[cfg(target_os = "macos")]
#[path = "pty/support.rs"]
mod support;

#[cfg(target_os = "macos")]
#[path = "pty/active_control.rs"]
mod active_control;

#[cfg(target_os = "macos")]
#[path = "pty/collapsed_entry.rs"]
mod collapsed_entry;

#[cfg(target_os = "macos")]
#[path = "pty/editor_persistence.rs"]
mod editor_persistence;

#[cfg(target_os = "macos")]
#[path = "pty/fairness.rs"]
mod fairness;

#[cfg(target_os = "macos")]
#[path = "pty/edit_submission.rs"]
mod edit_submission;

#[cfg(target_os = "macos")]
#[path = "pty/invocation.rs"]
mod invocation;

#[cfg(target_os = "macos")]
#[path = "pty/key_inspector.rs"]
mod key_inspector;

#[cfg(target_os = "macos")]
#[path = "pty/path_drop.rs"]
mod path_drop;

#[cfg(target_os = "macos")]
#[path = "pty/recovery.rs"]
mod recovery;

#[cfg(target_os = "macos")]
#[path = "pty/reorder.rs"]
mod reorder;

#[cfg(target_os = "macos")]
#[path = "pty/select_all.rs"]
mod select_all;

#[cfg(target_os = "macos")]
#[path = "pty/session_browser.rs"]
mod session_browser;

#[cfg(target_os = "macos")]
#[path = "pty/shutdown.rs"]
mod shutdown;

#[cfg(target_os = "macos")]
#[path = "pty/smart_lists.rs"]
mod smart_lists;

#[cfg(target_os = "macos")]
#[path = "pty/top_boundary.rs"]
mod top_boundary;

#[path = "pty/smoke.rs"]
mod smoke;

#[cfg(target_os = "macos")]
#[path = "pty/startup.rs"]
mod startup;

#[cfg(target_os = "macos")]
#[path = "pty/update_control.rs"]
mod update_control;

#[cfg(target_os = "macos")]
#[path = "pty/watchdog.rs"]
mod watchdog;
