#![cfg(unix)]
//! Registry for the Herdr plugin package: its build step, launcher, and the
//! real `proqi herdr toggle` binary driven by scripted Herdr and network fakes.

#[path = "herdr_plugin/install.rs"]
mod install;
#[path = "herdr_plugin/launcher.rs"]
mod launcher;
#[path = "herdr_plugin/support.rs"]
mod support;
#[path = "herdr_plugin/toggle.rs"]
mod toggle;
