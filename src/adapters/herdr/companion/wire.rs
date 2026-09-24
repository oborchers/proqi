//! Herdr plugin context, pane, and process shapes consumed by the companion host.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    domain::SessionId,
    ports::companion::{CompanionContext, PaneObservation, PaneProcess},
};

use super::LAUNCHER_PATH;

/// Display name Proqi publishes through its own pane metadata lease.
const PROQI_DISPLAY_AGENT: &str = "proqi";
/// Interactive shells a restored pane can start before any job runs.
const SHELLS: &[&str] = &[
    "bash", "dash", "fish", "ksh", "mksh", "nu", "sh", "tcsh", "zsh",
];

/// `HERDR_PLUGIN_CONTEXT_JSON` fields available since Herdr 0.8.0.
#[derive(Deserialize)]
pub(super) struct PluginContext {
    workspace_id: String,
    workspace_label: Option<String>,
    workspace_cwd: Option<PathBuf>,
    tab_id: String,
    tab_label: Option<String>,
    focused_pane_id: String,
    focused_pane_cwd: Option<PathBuf>,
}

impl PluginContext {
    pub(super) fn into_context(self) -> Option<CompanionContext> {
        let cwd = self
            .focused_pane_cwd
            .or_else(|| self.workspace_cwd.clone())?;
        Some(CompanionContext {
            workspace_id: self.workspace_id,
            workspace_label: self.workspace_label,
            tab_id: self.tab_id,
            tab_label: self.tab_label,
            focused_pane_id: self.focused_pane_id,
            focused_pane_cwd: cwd,
            workspace_cwd: self.workspace_cwd,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct PaneListBody {
    pub(super) panes: Vec<PaneWire>,
}

#[derive(Deserialize)]
pub(super) struct PaneWire {
    pane_id: String,
    pub(super) tab_id: String,
    #[serde(default)]
    focused: bool,
    agent: Option<String>,
    display_agent: Option<String>,
    label: Option<String>,
    cwd: Option<PathBuf>,
}

impl From<PaneWire> for PaneObservation {
    fn from(pane: PaneWire) -> Self {
        Self {
            proqi_presence: pane.display_agent.as_deref() == Some(PROQI_DISPLAY_AGENT),
            pane_id: pane.pane_id,
            focused: pane.focused,
            agent: pane.agent.is_some(),
            label: pane.label,
            cwd: pane.cwd,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct ProcessInfoBody {
    pub(super) process_info: ProcessInfo,
}

#[derive(Deserialize)]
pub(super) struct ProcessInfo {
    foreground_process_group_id: Option<u32>,
    #[serde(default)]
    foreground_processes: Vec<ForegroundProcess>,
    shell_pid: Option<u32>,
}

#[derive(Deserialize)]
struct ForegroundProcess {
    #[serde(default)]
    argv: Vec<String>,
    pid: u32,
}

#[derive(Deserialize)]
pub(super) struct PaneOpenedBody {
    pub(super) plugin_pane: OpenedPluginPane,
}

#[derive(Deserialize)]
pub(super) struct OpenedPluginPane {
    pub(super) pane: OpenedPane,
}

#[derive(Deserialize)]
pub(super) struct OpenedPane {
    pub(super) pane_id: String,
}

/// Reduce Herdr process information to the states the toggle may act on.
pub(super) fn classify(info: &ProcessInfo) -> PaneProcess {
    if let Some(process) = info
        .foreground_processes
        .iter()
        .find(|process| program_name(&process.argv) == Some("proqi"))
    {
        return PaneProcess::Proqi {
            session_id: resumed_session(&process.argv),
        };
    }
    // Matches both the manifest's `sh -c` wrapper and the exec'd launcher,
    // and nothing else that merely names the launcher, such as an editor.
    if info.foreground_processes.iter().any(|process| {
        program_name(&process.argv) == Some("sh")
            && process
                .argv
                .iter()
                .skip(1)
                .any(|argument| argument.contains(LAUNCHER_PATH))
    }) {
        return PaneProcess::Launcher;
    }
    if is_idle_shell(info) {
        PaneProcess::IdleShell
    } else {
        PaneProcess::Other
    }
}

fn is_idle_shell(info: &ProcessInfo) -> bool {
    let [process] = info.foreground_processes.as_slice() else {
        return false;
    };
    let shell = info.shell_pid == Some(process.pid)
        && info.foreground_process_group_id == Some(process.pid);
    shell
        && process.argv.len() == 1
        && program_name(&process.argv).is_some_and(|name| SHELLS.contains(&name))
}

/// Basename of `argv[0]` without a login-shell dash.
fn program_name(argv: &[String]) -> Option<&str> {
    let first = argv.first()?;
    let name = Path::new(first).file_name()?.to_str()?;
    Some(name.strip_prefix('-').unwrap_or(name))
}

fn resumed_session(argv: &[String]) -> Option<SessionId> {
    let mut arguments = argv.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let value = match argument.as_str() {
            "--resume" | "-r" => arguments.next().map(String::as_str),
            other => other.strip_prefix("--resume="),
        };
        if let Some(value) = value {
            return value.parse().ok();
        }
    }
    None
}
