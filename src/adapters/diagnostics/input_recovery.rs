//! Content-free input recovery diagnostic projection.

use crate::adapters::runtime::input_recovery::{
    RecoveryAdmission, RecoveryError, RecoveryRecordPhase,
};

#[derive(Debug, Eq, PartialEq)]
struct AdmissionFields {
    outcome: &'static str,
    reason: Option<&'static str>,
    retired_phase: Option<&'static str>,
    executable_changed: Option<bool>,
}

fn admission_fields(admission: Result<RecoveryAdmission, RecoveryError>) -> AdmissionFields {
    let (outcome, reason, retired_phase, executable_changed) = match admission {
        Ok(RecoveryAdmission::Fresh) => ("fresh", None, None, None),
        Ok(RecoveryAdmission::Probation) => ("probation", None, None, None),
        Ok(RecoveryAdmission::Retired {
            executable_changed,
            phase,
        }) => (
            "retired",
            None,
            Some(match phase {
                RecoveryRecordPhase::Prepared => "recovery_prepared",
                RecoveryRecordPhase::Probation => "probation",
                RecoveryRecordPhase::Healthy => "healthy",
            }),
            Some(executable_changed),
        ),
        Ok(RecoveryAdmission::Disabled(reason)) => ("disabled", Some(reason.as_str()), None, None),
        Err(error) => ("refused", Some(error.failure().as_str()), None, None),
    };
    AdmissionFields {
        outcome,
        reason,
        retired_phase,
        executable_changed,
    }
}

pub(crate) fn record_admission(admission: Result<RecoveryAdmission, RecoveryError>) {
    let fields = admission_fields(admission);
    tracing::info!(
        event = "input_recovery_admission",
        outcome = fields.outcome,
        reason = fields.reason,
        retired_phase = fields.retired_phase,
        executable_changed = fields.executable_changed,
    );
}

pub(super) fn record(
    stage: &str,
    reason: Option<&str>,
    attempt_count: usize,
    outcome: Option<&str>,
) {
    tracing::info!(
        event = "input_recovery",
        stage,
        reason,
        attempt_count,
        outcome
    );
}

#[cfg(test)]
mod tests {
    use super::{
        AdmissionFields, RecoveryAdmission, RecoveryError, RecoveryRecordPhase, admission_fields,
    };

    #[test]
    fn admission_projection_distinguishes_retirement_from_proof_without_identity() {
        assert_eq!(
            admission_fields(Ok(RecoveryAdmission::Retired {
                executable_changed: true,
                phase: RecoveryRecordPhase::Healthy,
            })),
            AdmissionFields {
                outcome: "retired",
                reason: None,
                retired_phase: Some("healthy"),
                executable_changed: Some(true),
            }
        );
        assert_eq!(
            admission_fields(Ok(RecoveryAdmission::Probation)).outcome,
            "probation"
        );
        assert_eq!(
            format!(
                "{:?}",
                admission_fields(Err(RecoveryError::MismatchedLineage))
            ),
            "AdmissionFields { outcome: \"refused\", reason: Some(\"mismatched_lineage\"), retired_phase: None, executable_changed: None }"
        );
    }
}
