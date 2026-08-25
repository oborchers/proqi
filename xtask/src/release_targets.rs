//! Canonical native release target registry used by repository automation.

pub(super) const ARM_MACOS: &str = "aarch64-apple-darwin";
pub(super) const INTEL_MACOS: &str = "x86_64-apple-darwin";
pub(super) const LINUX_X86_64: &str = "x86_64-unknown-linux-gnu";

pub(super) const ALL: [&str; 3] = [ARM_MACOS, INTEL_MACOS, LINUX_X86_64];

pub(super) fn archive_name(target: &str) -> String {
    format!("proqi-{target}.tar.gz")
}
