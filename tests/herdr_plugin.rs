#![cfg(unix)]
//! Registry for the Herdr plugin package: its build step, launcher, and the
//! real `proqi herdr toggle` and `proqi herdr capture` binaries driven by scripted
//! Herdr, clipboard, and network fakes.

#[path = "herdr_plugin/capture.rs"]
mod capture;
#[path = "herdr_plugin/fake_herdr.rs"]
mod fake_herdr;
#[path = "herdr_plugin/install.rs"]
mod install;
#[path = "herdr_plugin/launcher.rs"]
mod launcher;
#[path = "herdr_plugin/support.rs"]
mod support;
#[path = "herdr_plugin/toggle.rs"]
mod toggle;
