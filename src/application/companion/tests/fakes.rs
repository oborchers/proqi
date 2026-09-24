//! Deterministic host, plugin-state, and session fakes for the companion toggle.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    domain::SessionId,
    ports::companion::{
        CompanionContext, CompanionError, CompanionHost, CompanionRecord, CompanionRecords,
        CompanionSessionState, CompanionSessions, PaneObservation, PaneProcess,
    },
};

pub(super) fn session(id: &str) -> SessionId {
    id.parse().expect("session identifier")
}

pub(super) fn context(focused: &str) -> CompanionContext {
    CompanionContext {
        workspace_id: "w1".to_owned(),
        workspace_label: Some("demo".to_owned()),
        tab_id: "w1:t1".to_owned(),
        tab_label: Some("agent-tab".to_owned()),
        focused_pane_id: focused.to_owned(),
        focused_pane_cwd: PathBuf::from("/work"),
    }
}

pub(super) fn shell(pane: &str) -> PaneObservation {
    PaneObservation {
        pane_id: pane.to_owned(),
        focused: false,
        agent: false,
        proqi_presence: false,
        label: None,
        cwd: Some(PathBuf::from("/work")),
    }
}

pub(super) fn agent(pane: &str, focused: bool) -> PaneObservation {
    PaneObservation {
        focused,
        agent: true,
        ..shell(pane)
    }
}

pub(super) fn companion(pane: &str, focused: bool) -> PaneObservation {
    PaneObservation {
        focused,
        proqi_presence: true,
        label: Some(super::COMPANION_PANE_LABEL.to_owned()),
        ..shell(pane)
    }
}

pub(super) struct FakeHost {
    context: CompanionContext,
    panes: Vec<PaneObservation>,
    processes: HashMap<String, PaneProcess>,
    next_pane: u32,
    pub(super) calls: Vec<String>,
    pub(super) notifications: Vec<String>,
}

impl FakeHost {
    pub(super) fn new(focused: &str, panes: Vec<PaneObservation>) -> Self {
        Self {
            context: context(focused),
            panes,
            processes: HashMap::new(),
            next_pane: 10,
            calls: Vec::new(),
            notifications: Vec::new(),
        }
    }

    pub(super) fn with_process(mut self, pane: &str, process: PaneProcess) -> Self {
        self.processes.insert(pane.to_owned(), process);
        self
    }
}

impl CompanionHost for FakeHost {
    fn context(&mut self) -> Result<CompanionContext, CompanionError> {
        Ok(self.context.clone())
    }

    fn tab_panes(&mut self, tab_id: &str) -> Result<Vec<PaneObservation>, CompanionError> {
        assert_eq!(tab_id, "w1:t1");
        Ok(self.panes.clone())
    }

    fn process(&mut self, pane_id: &str) -> Result<Option<PaneProcess>, CompanionError> {
        Ok(self.processes.get(pane_id).cloned())
    }

    fn open_beside(
        &mut self,
        target_pane_id: &str,
        cwd: &Path,
        session_id: SessionId,
    ) -> Result<String, CompanionError> {
        self.calls.push(format!(
            "open {target_pane_id} {} {session_id}",
            cwd.display()
        ));
        let pane = format!("w1:p{}", self.next_pane);
        self.next_pane += 1;
        Ok(pane)
    }

    fn focus(&mut self, pane_id: &str) -> Result<(), CompanionError> {
        self.calls.push(format!("focus {pane_id}"));
        Ok(())
    }

    fn close(&mut self, pane_id: &str) -> Result<(), CompanionError> {
        self.calls.push(format!("close {pane_id}"));
        Ok(())
    }

    fn notify(&mut self, message: &str) {
        self.notifications.push(message.to_owned());
    }
}

#[derive(Default)]
pub(super) struct FakeRecords(pub(super) Vec<CompanionRecord>);

impl CompanionRecords for FakeRecords {
    fn all(&mut self) -> Result<Vec<CompanionRecord>, CompanionError> {
        Ok(self.0.clone())
    }

    fn save(&mut self, record: &CompanionRecord) -> Result<(), CompanionError> {
        self.0.retain(|existing| existing.tab_id != record.tab_id);
        self.0.push(record.clone());
        Ok(())
    }

    fn remove(&mut self, tab_id: &str) -> Result<(), CompanionError> {
        self.0.retain(|existing| existing.tab_id != tab_id);
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct FakeSessions {
    named: HashMap<String, SessionId>,
    states: HashMap<SessionId, CompanionSessionState>,
    pub(super) fail_flush: bool,
    pub(super) ensured: Vec<(String, PathBuf)>,
    pub(super) flushed: Vec<SessionId>,
}

impl FakeSessions {
    pub(super) fn with_named(name: &str, id: &str) -> Self {
        let mut sessions = Self::default();
        sessions.named.insert(name.to_owned(), session(id));
        sessions
    }

    pub(super) fn with_state(mut self, id: &str, state: CompanionSessionState) -> Self {
        self.states.insert(session(id), state);
        self
    }
}

impl CompanionSessions for FakeSessions {
    type Error = String;

    fn ensure(&mut self, name: &str, cwd: &Path) -> Result<SessionId, String> {
        self.ensured.push((name.to_owned(), cwd.to_path_buf()));
        self.named
            .get(name)
            .copied()
            .ok_or_else(|| format!("no fake session named {name}"))
    }

    fn state(&mut self, session_id: SessionId) -> Result<CompanionSessionState, String> {
        Ok(self
            .states
            .get(&session_id)
            .copied()
            .unwrap_or(CompanionSessionState::Resumable))
    }

    fn flush(&mut self, session_id: SessionId) -> Result<(), String> {
        if self.fail_flush {
            return Err("owner did not confirm the flush".to_owned());
        }
        self.flushed.push(session_id);
        Ok(())
    }
}
