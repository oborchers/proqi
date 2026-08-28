//! GitHub release, installation detection, and private update-state adapters.

mod cache;
mod etag;
mod github;
mod installation;
mod installer;

pub use cache::FileUpdateStateStore;
pub use github::GitHubReleaseSource;
pub use installation::SystemInstallDetector;
pub use installer::HomebrewFormulaInstaller;
pub(crate) use installer::verify_installed_version;
