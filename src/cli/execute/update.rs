use crate::cli::error_code::ErrorCode;
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
            ErrorCode::InstalledVersionInvalid,
            "installed Proqi version is invalid".to_owned(),
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
    let (code, message) = match error {
        UpdateError::Network => (
            ErrorCode::UpdateNetworkFailed,
            "stable release check failed",
        ),
        UpdateError::InvalidResponse => (
            ErrorCode::UpdateResponseInvalid,
            "stable release response is invalid",
        ),
        UpdateError::ResponseTooLarge => (
            ErrorCode::UpdateResponseTooLarge,
            "stable release response exceeded its limit",
        ),
        UpdateError::Installation(_) => (
            ErrorCode::InstallationUnverified,
            "installation context could not be verified",
        ),
        UpdateError::State(_) => (
            ErrorCode::UpdateStateFailed,
            "private update state could not be accessed",
        ),
        UpdateError::Coordination(_) => (
            ErrorCode::UpdateCoordinationFailed,
            "active Proqi sessions could not be coordinated",
        ),
        UpdateError::InstallerFailed => (
            ErrorCode::UpdateInstallationFailed,
            "the verified installation method could not install the release",
        ),
    };
    CliError::new(code, message.to_owned())
}
