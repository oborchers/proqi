//! Read-only health-check command presentation.

use crate::cli::error_code::ErrorCode;
use serde_json::json;

use super::Outcome;
use crate::{
    adapters::doctor::{DoctorReport, DoctorStatus, inspect},
    cli::output::CliError,
    ports::environment::AppPaths,
};

pub(super) fn execute(paths: &AppPaths) -> Result<Outcome, CliError> {
    let report = inspect(paths);
    let human = render_human(&report);
    if report.overall_status == DoctorStatus::Fail {
        return Err(CliError::new(ErrorCode::DoctorFailed, human).with_details(json!(report)));
    }
    Ok(Outcome {
        data: json!(report),
        human,
    })
}

fn render_human(report: &DoctorReport) -> String {
    let mut lines = vec![format!("Proqi doctor: {}", report.overall_status)];
    for check in &report.checks {
        lines.push(format!(
            "{:<7} {:<20} {}",
            check.status, check.id, check.summary
        ));
        if let Some(remediation) = &check.remediation {
            lines.push(format!("        {remediation}"));
        }
    }
    lines.join("\n")
}
