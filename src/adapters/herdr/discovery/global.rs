//! Fresh current-server compatible-agent discovery.

use std::collections::{BTreeMap, BTreeSet};

use crate::ports::{
    agent::{
        AgentAvailability, AgentDeliveryCapabilities, AgentError, AgentState, AgentTarget,
        HerdrAgentAddress,
    },
    environment::ProcessRunner,
};

use super::{
    super::{
        DISCOVERY_TIMEOUT, HerdrGateway,
        compatibility::HerdrCompatibilityPolicy,
        contract::{CurrentBody, Envelope, PaneInfo, SchemaDocument, SnapshotBody},
    },
    MAX_AGENT_ROWS, observed_state,
    topology::{
        correlated_tab_label, correlated_workspace_label, sanitize_agent_name, tab_labels,
        workspace_labels,
    },
};

pub(super) fn targets<R: ProcessRunner>(
    gateway: &mut HerdrGateway<R>,
) -> Result<Vec<AgentTarget>, AgentError> {
    if !gateway.managed {
        return Err(AgentError::Unavailable(
            "HERDR_ENV is not set for this pane".to_owned(),
        ));
    }
    let schema: SchemaDocument = gateway.json(&["api", "schema", "--json"], DISCOVERY_TIMEOUT)?;
    let body: Envelope<SnapshotBody> = gateway.json(&["api", "snapshot"], DISCOVERY_TIMEOUT)?;
    let snapshot = body.result.snapshot;
    let protocol = HerdrCompatibilityPolicy::negotiate(&schema, &snapshot)?.value();
    let current: Envelope<CurrentBody> =
        gateway.json(&["pane", "current", "--current"], DISCOVERY_TIMEOUT)?;
    if current.result.pane.pane_id.trim().is_empty() {
        return Err(AgentError::Malformed(
            "current Herdr pane has an incomplete identity".to_owned(),
        ));
    }
    if snapshot.agents.len() > MAX_AGENT_ROWS
        || snapshot.workspaces.len() > MAX_AGENT_ROWS
        || snapshot.tabs.len() > MAX_AGENT_ROWS
    {
        return Err(AgentError::Unsupported(
            "current-server agent discovery exceeds the verified row budget".to_owned(),
        ));
    }
    let workspaces = workspace_labels(snapshot.workspaces, MAX_AGENT_ROWS)?;
    let tabs = tab_labels(snapshot.tabs, MAX_AGENT_ROWS)?;
    unique_targets(
        snapshot.agents,
        protocol,
        &current.result.pane.pane_id,
        &workspaces,
        &tabs,
    )
}

fn unique_targets(
    agents: Vec<PaneInfo>,
    protocol: u32,
    current_pane_id: &str,
    workspaces: &BTreeMap<String, Option<String>>,
    tabs: &BTreeMap<String, (String, Option<String>)>,
) -> Result<Vec<AgentTarget>, AgentError> {
    let mut identities = BTreeSet::new();
    let mut pane_ids = BTreeSet::new();
    let mut targets = Vec::new();
    for pane in agents {
        if pane.pane_id == current_pane_id {
            continue;
        }
        let Some(target) = target(&pane, protocol, workspaces, tabs)? else {
            continue;
        };
        let identity = (
            target.workspace_id().to_owned(),
            target.tab_id().to_owned(),
            target.pane_id().to_owned(),
            target.agent_kind().as_str().to_owned(),
            target.agent_session().as_id().map(str::to_owned),
        );
        if !pane_ids.insert(target.pane_id().to_owned()) || !identities.insert(identity) {
            return Err(AgentError::Ambiguous(
                "multiple compatible agents claim one delivery identity".to_owned(),
            ));
        }
        targets.push(target);
    }
    Ok(targets)
}

fn target(
    pane: &PaneInfo,
    protocol: u32,
    workspaces: &BTreeMap<String, Option<String>>,
    tabs: &BTreeMap<String, (String, Option<String>)>,
) -> Result<Option<AgentTarget>, AgentError> {
    let Some(kind) = pane
        .agent
        .clone()
        .and_then(|value| super::super::harness::kind(value).ok())
    else {
        return Ok(None);
    };
    let agent_session =
        match super::super::harness::discovered_session(&kind, pane.agent_session.as_ref()) {
            Ok(session) => session,
            Err(AgentError::Unsupported(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
    if pane.workspace_id.trim().is_empty()
        || pane.tab_id.trim().is_empty()
        || pane.pane_id.trim().is_empty()
    {
        return Err(AgentError::Malformed(
            "compatible agent has an incomplete delivery identity".to_owned(),
        ));
    }
    let workspace_label = correlated_workspace_label(workspaces, &pane.workspace_id, false)?;
    let tab_label = correlated_tab_label(tabs, &pane.workspace_id, &pane.tab_id, false)?;
    let readiness = observed_state(pane.agent_status);
    let availability = availability(pane, readiness);
    let agent_name = pane
        .name
        .as_deref()
        .map(sanitize_agent_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("{kind} agent"));
    Ok(Some(AgentTarget::herdr_agent(
        protocol,
        HerdrAgentAddress::new(
            pane.workspace_id.clone(),
            pane.tab_id.clone(),
            pane.pane_id.clone(),
            kind,
            agent_session,
        )
        .ok_or_else(|| AgentError::Malformed("invalid current-server agent address".to_owned()))?,
        agent_name,
        workspace_label,
        tab_label,
        readiness,
        availability,
        AgentDeliveryCapabilities::SUBMIT_ONLY,
    )))
}

fn availability(pane: &PaneInfo, state: AgentState) -> AgentAvailability {
    if pane.launch_pending == Some(true) {
        AgentAvailability::Launching
    } else if pane.interactive_ready == Some(false) {
        AgentAvailability::NotInteractive
    } else {
        match state {
            AgentState::Idle | AgentState::Working | AgentState::Done => {
                AgentAvailability::Available
            }
            AgentState::Blocked => AgentAvailability::Blocked,
            AgentState::Unknown => AgentAvailability::Unknown,
        }
    }
}
