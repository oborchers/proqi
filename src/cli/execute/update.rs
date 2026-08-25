use std::path::Path;

use serde_json::json;

use crate::{
    adapters::{
        runtime::SystemClock,
        update::{FileUpdateStateStore, GitHubReleaseSource, SystemInstallDetector},
    },
    application::{
        UpdateAvailability, UpdateCheckMode, UpdateCheckResult, UpdateRefresh, UpdateService,
    },
    domain::StableVersion,
    ports::update::UpdateError,
};

use super::{
    super::{
        args::{UpdateArgs, UpdateCommand},
        output::CliError,
    },
    Outcome,
};

pub(super) fn execute(arguments: &UpdateArgs, cache_dir: &Path) -> Result<Outcome, CliError> {
    match arguments.command {
        UpdateCommand::Check => check(cache_dir),
    }
}

fn check(cache_dir: &Path) -> Result<Outcome, CliError> {
    let installed = StableVersion::parse(env!("CARGO_PKG_VERSION")).map_err(|_| {
        CliError::new(
            "installed_version_invalid",
            "installed Proqi version is invalid".to_owned(),
            1,
        )
    })?;
    let store = FileUpdateStateStore::new(cache_dir).map_err(|error| update_error(&error))?;
    let mut source = GitHubReleaseSource::new();
    let detector = SystemInstallDetector::current();
    let clock = SystemClock;
    let result = UpdateService::new(&store, &mut source, &detector, &clock)
        .check(installed, UpdateCheckMode::Explicit)
        .map_err(|error| update_error(&error))?;
    Ok(outcome(&result))
}

fn outcome(result: &UpdateCheckResult) -> Outcome {
    let human = if result.refresh == UpdateRefresh::InProgress {
        "Another Proqi process is checking for updates".to_owned()
    } else if result.availability != UpdateAvailability::Current {
        format!(
            "Proqi {} is available (installed {})\n{}",
            result
                .latest_version
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), ToString::to_string),
            result.installed_version,
            result
                .release_url
                .unwrap_or("https://github.com/oborchers/proqi/releases/latest")
        )
    } else {
        format!("Proqi {} is up to date", result.installed_version)
    };
    Outcome {
        data: json!(result),
        human,
    }
}

fn update_error(error: &UpdateError) -> CliError {
    let (code, message, exit) = match error {
        UpdateError::Network => ("update_network_failed", "stable release check failed", 1),
        UpdateError::InvalidResponse => (
            "update_response_invalid",
            "stable release response is invalid",
            1,
        ),
        UpdateError::ResponseTooLarge => (
            "update_response_too_large",
            "stable release response exceeded its limit",
            1,
        ),
        UpdateError::Installation(_) => (
            "installation_unverified",
            "installation context could not be verified",
            1,
        ),
        UpdateError::State(_) => (
            "update_state_failed",
            "private update state could not be accessed",
            1,
        ),
        UpdateError::Coordination(_) => (
            "update_coordination_failed",
            "active Proqi sessions could not be coordinated",
            1,
        ),
        UpdateError::InstallerFailed => (
            "update_installation_failed",
            "Homebrew could not install the verified release",
            1,
        ),
    };
    CliError::new(code, message.to_owned(), exit)
}
