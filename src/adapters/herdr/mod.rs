//! Optional, fail-closed Herdr semantic prompt adapter.

mod contract;
mod discovery;
mod submission;
#[cfg(test)]
mod tests;

use std::{ffi::OsString, time::Duration};

use serde::de::DeserializeOwned;

use crate::ports::{
    agent::{
        AgentCapabilities, AgentError, AgentGateway, AgentTarget, PaneContext, PanePresentation,
        SubmissionReceipt, SubmissionRequest,
    },
    environment::{ProcessError, ProcessRequest, ProcessRunner},
};

use contract::ErrorEnvelope;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);
const SUBMISSION_TIMEOUT: Duration = Duration::from_secs(5);
const SUPPORTED_PROTOCOL: u32 = 19;
const SUPPORTED_SCHEMA: u32 = 1;

/// Herdr gateway using direct, bounded child-process calls.
pub struct HerdrGateway<R> {
    runner: R,
    program: OsString,
    managed: bool,
    presentation_source: String,
}

impl<R> HerdrGateway<R> {
    /// Construct an injectable gateway for composition or contract tests.
    #[must_use]
    pub fn new(program: OsString, runner: R, managed: bool) -> Self {
        Self {
            runner,
            program,
            managed,
            presentation_source: "proqi".to_owned(),
        }
    }

    /// Use one process-unique source for monotonic display metadata sequences.
    #[must_use]
    pub fn with_presentation_source(mut self, source: String) -> Self {
        self.presentation_source = source;
        self
    }
}

impl HerdrGateway<crate::adapters::process::SystemProcessRunner> {
    /// Compose the installed Herdr binary and inherited managed-pane context.
    #[must_use]
    pub fn from_environment(presentation_source: String) -> Self {
        Self::from_environment_with_runner(
            presentation_source,
            crate::adapters::process::SystemProcessRunner::default(),
        )
    }

    pub(crate) fn from_environment_with_runner(
        presentation_source: String,
        runner: crate::adapters::process::SystemProcessRunner,
    ) -> Self {
        let managed = std::env::var_os("HERDR_ENV").is_some_and(|value| value == "1")
            && std::env::var_os("PROQI_DISABLE_HERDR").is_none();
        Self::new(OsString::from("herdr"), runner, managed)
            .with_presentation_source(presentation_source)
    }
}

impl<R: ProcessRunner> HerdrGateway<R> {
    fn json<T: DeserializeOwned>(
        &mut self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<T, AgentError> {
        let output = self
            .runner
            .run(ProcessRequest {
                program: self.program.clone(),
                args: args.iter().map(OsString::from).collect(),
                stdin: None,
                timeout,
            })
            .map_err(process_error)?;
        if output.exit_code != Some(0) {
            return Err(command_error(&output.stderr));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| AgentError::Malformed(error.to_string()))
    }
}

impl<R: ProcessRunner> AgentGateway for HerdrGateway<R> {
    fn capabilities(&mut self) -> Result<AgentCapabilities, AgentError> {
        discovery::capabilities(self)
    }

    fn adjacent_targets(&mut self, context: &PaneContext) -> Result<Vec<AgentTarget>, AgentError> {
        discovery::adjacent_targets(self, context)
    }

    fn submit(&mut self, request: SubmissionRequest) -> Result<SubmissionReceipt, AgentError> {
        submission::submit(self, &request)
    }
}

impl<R: ProcessRunner> PanePresentation for HerdrGateway<R> {
    fn publish(&mut self, pane_id: &str, sequence: u64, ttl: Duration) -> Result<(), AgentError> {
        let sequence = sequence.to_string();
        let ttl = ttl.as_millis().to_string();
        let source = self.presentation_source.clone();
        self.metadata(&[
            pane_id,
            "--source",
            &source,
            "--title",
            "proqi",
            "--display-agent",
            "proqi",
            "--seq",
            &sequence,
            "--ttl-ms",
            &ttl,
        ])
    }

    fn clear(&mut self, pane_id: &str, sequence: u64) -> Result<(), AgentError> {
        let sequence = sequence.to_string();
        let source = self.presentation_source.clone();
        self.metadata(&[
            pane_id,
            "--source",
            &source,
            "--clear-title",
            "--clear-display-agent",
            "--seq",
            &sequence,
        ])
    }
}

impl<R: ProcessRunner> HerdrGateway<R> {
    fn metadata(&mut self, args: &[&str]) -> Result<(), AgentError> {
        if !self.managed {
            return Err(AgentError::Unavailable(
                "HERDR_ENV is not set for this pane".to_owned(),
            ));
        }
        let mut command = vec![OsString::from("pane"), OsString::from("report-metadata")];
        command.extend(args.iter().map(OsString::from));
        let output = self
            .runner
            .run(ProcessRequest {
                program: self.program.clone(),
                args: command,
                stdin: None,
                timeout: DISCOVERY_TIMEOUT,
            })
            .map_err(process_error)?;
        if output.exit_code == Some(0) {
            Ok(())
        } else {
            Err(command_error(&output.stderr))
        }
    }
}

fn process_error(error: ProcessError) -> AgentError {
    match error {
        ProcessError::TimedOut => AgentError::TimedOut,
        ProcessError::Cancelled => AgentError::Process("process cancelled".to_owned()),
        ProcessError::Io(message) => AgentError::Process(message),
        ProcessError::OutputLimit => AgentError::Malformed("provider output exceeded limit".into()),
    }
}

fn command_error(stderr: &[u8]) -> AgentError {
    serde_json::from_slice::<ErrorEnvelope>(stderr).map_or_else(
        |_| AgentError::Process(String::from_utf8_lossy(stderr).trim().to_owned()),
        |response| AgentError::Rejected {
            code: response.error.code,
            message: response.error.message,
        },
    )
}
