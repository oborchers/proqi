//! Herdr plugin implementation of the companion-pane host.
//!
//! Every call is one direct, bounded Herdr CLI process without a shell. The
//! manifest entrypoint receives the session identity through one environment
//! variable and replaces its launcher with `proqi --resume`.

mod state;
#[cfg(test)]
mod tests;
mod wire;

use std::{ffi::OsString, path::Path, time::Duration};

use serde::de::DeserializeOwned;

use crate::{
    domain::SessionId,
    ports::{
        companion::{
            CompanionContext, CompanionError, CompanionHost, PaneObservation, PaneProcess,
        },
        environment::{ProcessRequest, ProcessRunner},
    },
};

pub use state::FileCompanionRecords;

use super::contract::{Envelope, ErrorEnvelope};

/// Plugin identity declared by the repository manifest.
pub const PLUGIN_ID: &str = "proqi";
/// Manifest action that toggles the companion.
pub const TOGGLE_ACTION_ID: &str = "toggle";
/// Manifest pane entrypoint that resumes one session.
pub const PANE_ENTRYPOINT_ID: &str = "board";
/// Environment variable carrying the session identity into the pane entrypoint.
pub const SESSION_ENVIRONMENT: &str = "PROQI_HERDR_SESSION";
/// Plugin-relative launcher shared by the action and the pane entrypoint.
pub const LAUNCHER_PATH: &str = "herdr-plugin/proqi.sh";
/// Plugin-relative build step that installs Proqi only when it is absent.
pub const INSTALL_PATH: &str = "herdr-plugin/install.sh";
/// Oldest Herdr release whose protocol Proqi qualifies and whose plugin surface this host uses.
pub const MIN_HERDR_VERSION: &str = "0.8.0";

const QUERY_TIMEOUT: Duration = Duration::from_secs(3);
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const NOTIFICATION_TITLE: &str = "Proqi";

/// Herdr plugin action environment, read once at composition.
#[derive(Clone, Debug)]
pub struct HerdrPluginEnvironment {
    program: OsString,
    plugin_id: String,
    context: String,
    state_dir: std::path::PathBuf,
}

impl HerdrPluginEnvironment {
    /// Read the variables Herdr injects into plugin action commands.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError::Unavailable`] outside a Herdr plugin action.
    pub fn detect() -> Result<Self, CompanionError> {
        let required = |name: &str| {
            std::env::var(name).ok().filter(|value| !value.is_empty()).ok_or_else(|| {
                CompanionError::Unavailable(format!(
                    "proqi herdr toggle runs only as the Proqi Herdr plugin action ({name} is not set)"
                ))
            })
        };
        if required("HERDR_ENV")? != "1" {
            return Err(CompanionError::Unavailable(
                "proqi herdr toggle runs only inside Herdr".to_owned(),
            ));
        }
        Ok(Self {
            program: std::env::var_os("HERDR_BIN_PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| OsString::from("herdr")),
            plugin_id: required("HERDR_PLUGIN_ID")?,
            context: required("HERDR_PLUGIN_CONTEXT_JSON")?,
            state_dir: required("HERDR_PLUGIN_STATE_DIR")?.into(),
        })
    }

    /// Construct an explicit environment for tests and alternate composition.
    #[must_use]
    pub fn new(
        program: OsString,
        plugin_id: String,
        context: String,
        state_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            program,
            plugin_id,
            context,
            state_dir,
        }
    }

    /// Private plugin state directory owned by Herdr for this plugin.
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }
}

/// Companion host over the installed Herdr CLI.
pub struct HerdrCompanionHost<R> {
    runner: R,
    environment: HerdrPluginEnvironment,
}

impl<R> HerdrCompanionHost<R> {
    /// Compose a host with an injectable process runner.
    #[must_use]
    pub fn new(environment: HerdrPluginEnvironment, runner: R) -> Self {
        Self {
            runner,
            environment,
        }
    }
}

impl<R: ProcessRunner> HerdrCompanionHost<R> {
    fn run(&mut self, args: &[&str], timeout: Duration) -> Result<Vec<u8>, HostFailure> {
        let output = self
            .runner
            .run(ProcessRequest {
                program: self.environment.program.clone(),
                args: args.iter().map(OsString::from).collect(),
                stdin: None,
                timeout,
            })
            .map_err(|error| HostFailure::Process(format!("{error:?}")))?;
        if output.exit_code == Some(0) {
            return Ok(output.stdout);
        }
        let envelope = serde_json::from_slice::<ErrorEnvelope>(&output.stderr)
            .or_else(|_| serde_json::from_slice::<ErrorEnvelope>(&output.stdout));
        Err(match envelope {
            Ok(envelope) => HostFailure::Rejected {
                code: envelope.error.code,
                message: envelope.error.message,
            },
            Err(_) => {
                HostFailure::Process(String::from_utf8_lossy(&output.stderr).trim().to_owned())
            }
        })
    }

    fn json<T: DeserializeOwned>(
        &mut self,
        args: &[&str],
        timeout: Duration,
    ) -> Result<T, HostFailure> {
        let stdout = self.run(args, timeout)?;
        serde_json::from_slice::<Envelope<T>>(&stdout)
            .map(|envelope| envelope.result)
            .map_err(|error| HostFailure::Process(format!("malformed Herdr response: {error}")))
    }

    fn focus_any(&mut self, pane_id: &str) -> Result<(), HostFailure> {
        match self.run(&["plugin", "pane", "focus", pane_id], QUERY_TIMEOUT) {
            Ok(_) => return Ok(()),
            Err(HostFailure::Rejected { .. }) => {}
            Err(error) => return Err(error),
        }
        if self
            .run(&["agent", "focus", pane_id], QUERY_TIMEOUT)
            .is_ok()
        {
            return Ok(());
        }
        // Herdr 0.8.0 through 0.9.1 expose no focus-by-id for an arbitrary pane;
        // zooming one focuses it, and the second call restores the layout.
        self.run(&["pane", "zoom", pane_id, "--on"], QUERY_TIMEOUT)?;
        self.run(&["pane", "zoom", pane_id, "--off"], QUERY_TIMEOUT)
            .map(|_| ())
    }
}

impl<R: ProcessRunner> CompanionHost for HerdrCompanionHost<R> {
    fn context(&mut self) -> Result<CompanionContext, CompanionError> {
        serde_json::from_str::<wire::PluginContext>(&self.environment.context)
            .ok()
            .and_then(wire::PluginContext::into_context)
            .ok_or_else(|| {
                CompanionError::Unavailable(
                    "Herdr did not supply a focused pane and tab for this plugin action".to_owned(),
                )
            })
    }

    fn tab_panes(&mut self, tab_id: &str) -> Result<Vec<PaneObservation>, CompanionError> {
        let body: wire::PaneListBody = self.json(&["pane", "list"], QUERY_TIMEOUT)?;
        let mut panes: Vec<PaneObservation> = body
            .panes
            .into_iter()
            .filter(|pane| pane.tab_id == tab_id)
            .map(PaneObservation::from)
            .collect();
        // A Proqi that cannot reach Herdr publishes no display lease. Recognize
        // it by its foreground process so the toggle focuses it instead of
        // opening a second one. Recognition never makes the pane closable.
        for pane in panes
            .iter_mut()
            .filter(|pane| !pane.proqi_presence && !pane.agent)
        {
            if let Some(PaneProcess::Proqi { .. }) = self.process(&pane.pane_id)? {
                pane.proqi_presence = true;
            }
        }
        Ok(panes)
    }

    fn process(&mut self, pane_id: &str) -> Result<Option<PaneProcess>, CompanionError> {
        match self.json::<wire::ProcessInfoBody>(
            &["pane", "process-info", "--pane", pane_id],
            QUERY_TIMEOUT,
        ) {
            Ok(body) => Ok(Some(wire::classify(&body.process_info))),
            Err(HostFailure::Rejected { code, .. }) if code == "pane_not_found" => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn open_beside(
        &mut self,
        target_pane_id: &str,
        cwd: &Path,
        session_id: SessionId,
    ) -> Result<String, CompanionError> {
        let cwd = cwd.to_str().ok_or_else(|| {
            CompanionError::Host("the agent pane directory is not valid UTF-8".to_owned())
        })?;
        let plugin_id = self.environment.plugin_id.clone();
        let session = format!("{SESSION_ENVIRONMENT}={session_id}");
        let body: wire::PaneOpenedBody = self.json(
            &[
                "plugin",
                "pane",
                "open",
                "--plugin",
                &plugin_id,
                "--entrypoint",
                PANE_ENTRYPOINT_ID,
                "--placement",
                "split",
                "--target-pane",
                target_pane_id,
                "--direction",
                "right",
                "--cwd",
                cwd,
                "--env",
                &session,
                "--focus",
            ],
            OPEN_TIMEOUT,
        )?;
        Ok(body.plugin_pane.pane.pane_id)
    }

    fn focus(&mut self, pane_id: &str) -> Result<(), CompanionError> {
        self.focus_any(pane_id).map_err(Into::into)
    }

    fn close(&mut self, pane_id: &str) -> Result<(), CompanionError> {
        match self.run(&["pane", "close", pane_id], QUERY_TIMEOUT) {
            Ok(_) => Ok(()),
            Err(HostFailure::Rejected { code, .. }) if code == "pane_not_found" => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn notify(&mut self, message: &str) {
        let _best_effort = self.run(
            &[
                "notification",
                "show",
                NOTIFICATION_TITLE,
                "--body",
                message,
            ],
            QUERY_TIMEOUT,
        );
    }
}

/// Adapter-local Herdr failure before translation into the port error.
#[derive(Debug)]
enum HostFailure {
    Process(String),
    Rejected { code: String, message: String },
}

impl From<HostFailure> for CompanionError {
    fn from(failure: HostFailure) -> Self {
        match failure {
            HostFailure::Process(message) => Self::Host(format!("Herdr request failed: {message}")),
            HostFailure::Rejected { code, message } => {
                Self::Host(format!("Herdr rejected the request ({code}): {message}"))
            }
        }
    }
}
