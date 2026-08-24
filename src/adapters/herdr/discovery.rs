//! Capability negotiation and independently verified directional lookup.

use crate::{
    domain::Direction,
    ports::{
        agent::{
            AgentCapabilities, AgentDeliveryCapabilities, AgentError, AgentReadiness, AgentTarget,
            PaneContext, PaneRect,
        },
        environment::ProcessRunner,
    },
};

use super::{
    DISCOVERY_TIMEOUT, HerdrGateway, SUPPORTED_PROTOCOL, SUPPORTED_SCHEMA,
    contract::{
        AgentsBody, CurrentBody, Envelope, LayoutBody, NeighborBody, PaneInfo, RawReadiness,
        SchemaDocument, SnapshotBody,
    },
};

pub(super) fn capabilities<R: ProcessRunner>(
    gateway: &mut HerdrGateway<R>,
) -> Result<AgentCapabilities, AgentError> {
    if !gateway.managed {
        return Err(AgentError::Unavailable(
            "HERDR_ENV is not set for this pane".to_owned(),
        ));
    }
    let schema: SchemaDocument = gateway.json(&["api", "schema", "--json"], DISCOVERY_TIMEOUT)?;
    let live: Envelope<SnapshotBody> = gateway.json(&["api", "snapshot"], DISCOVERY_TIMEOUT)?;
    verify_protocol(&schema, &live.result.snapshot)?;
    let current: Envelope<CurrentBody> =
        gateway.json(&["pane", "current", "--current"], DISCOVERY_TIMEOUT)?;
    let layout: Envelope<LayoutBody> = gateway.json(
        &["pane", "layout", "--pane", &current.result.pane.pane_id],
        DISCOVERY_TIMEOUT,
    )?;
    let context = context_from(&current.result.pane, &layout.result.layout)?;
    Ok(AgentCapabilities {
        provider: "herdr".to_owned(),
        version: live.result.snapshot.version,
        protocol: schema.protocol,
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        context,
    })
}

pub(super) fn adjacent_targets<R: ProcessRunner>(
    gateway: &mut HerdrGateway<R>,
    expected: &PaneContext,
) -> Result<Vec<AgentTarget>, AgentError> {
    let current = capabilities(gateway)?;
    if &current.context != expected {
        return Err(AgentError::Unsupported(
            "current Herdr pane changed since discovery".to_owned(),
        ));
    }
    let agents: Envelope<AgentsBody> = gateway.json(&["agent", "list"], DISCOVERY_TIMEOUT)?;
    let mut targets = Vec::new();
    for direction in directions() {
        let word = direction_name(direction);
        let neighbor: Envelope<NeighborBody> = gateway.json(
            &[
                "pane",
                "neighbor",
                "--pane",
                &expected.pane_id,
                "--direction",
                word,
            ],
            DISCOVERY_TIMEOUT,
        )?;
        if let Some(target) = verify_neighbor(expected, direction, neighbor.result, &agents.result)?
        {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn verify_protocol(
    schema: &SchemaDocument,
    live: &super::contract::Snapshot,
) -> Result<(), AgentError> {
    if schema.schema_version != SUPPORTED_SCHEMA
        || schema.protocol != SUPPORTED_PROTOCOL
        || live.protocol != schema.protocol
        || live.version.trim().is_empty()
        || !contains_const(&schema.schemas, "agent.prompt")
        || !contains_const(&schema.schemas, "agent_prompted")
    {
        return Err(AgentError::Unsupported(format!(
            "requires schema {SUPPORTED_SCHEMA} and protocol {SUPPORTED_PROTOCOL}, received schema {} and protocols {}/{}",
            schema.schema_version, schema.protocol, live.protocol
        )));
    }
    Ok(())
}

fn contains_const(value: &serde_json::Value, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields.get("const").and_then(serde_json::Value::as_str) == Some(expected)
                || fields.values().any(|value| contains_const(value, expected))
        }
        serde_json::Value::Array(values) => {
            values.iter().any(|value| contains_const(value, expected))
        }
        _ => false,
    }
}

fn context_from(
    current: &PaneInfo,
    layout: &super::contract::PaneLayout,
) -> Result<PaneContext, AgentError> {
    verify_layout_identity(current, layout)?;
    let rect = unique_rect(layout, &current.pane_id)?;
    Ok(PaneContext {
        workspace_id: current.workspace_id.clone(),
        tab_id: current.tab_id.clone(),
        pane_id: current.pane_id.clone(),
        rect,
    })
}

fn verify_neighbor(
    source: &PaneContext,
    direction: Direction,
    body: NeighborBody,
    agents: &AgentsBody,
) -> Result<Option<AgentTarget>, AgentError> {
    let neighbor = body.neighbor;
    verify_neighbor_context(source, direction, &neighbor)?;
    let Some(pane_id) = neighbor.candidate_pane_id else {
        return Ok(None);
    };
    if pane_id == source.pane_id {
        return Err(AgentError::Malformed(
            "neighbor equals source pane".to_owned(),
        ));
    }
    let rect = unique_rect(&neighbor.layout, &pane_id)?;
    if !is_adjacent(source.rect, rect, direction) {
        return Err(AgentError::Malformed(format!(
            "{pane_id} is not geometrically adjacent {direction:?}"
        )));
    }
    let agent = unique_agent(&agents.agents, &pane_id)?;
    match eligible_target(source, direction, rect, agent) {
        Ok(target) => Ok(Some(target)),
        Err(AgentError::Unsupported(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn verify_neighbor_context(
    source: &PaneContext,
    direction: Direction,
    neighbor: &super::contract::Neighbor,
) -> Result<(), AgentError> {
    if neighbor.pane_id != source.pane_id
        || neighbor.direction != direction
        || neighbor.layout.workspace_id != source.workspace_id
        || neighbor.layout.tab_id != source.tab_id
        || unique_rect(&neighbor.layout, &source.pane_id)? != source.rect
    {
        return Err(AgentError::Malformed(
            "directional response does not match source context".to_owned(),
        ));
    }
    Ok(())
}

fn eligible_target(
    source: &PaneContext,
    direction: Direction,
    rect: PaneRect,
    agent: &PaneInfo,
) -> Result<AgentTarget, AgentError> {
    if agent.workspace_id != source.workspace_id || agent.tab_id != source.tab_id {
        return Err(AgentError::Malformed(
            "agent identity belongs to another workspace or tab".to_owned(),
        ));
    }
    if agent.interactive_ready == Some(false) || agent.launch_pending == Some(true) {
        return Err(AgentError::Unsupported(
            "neighbor is not ready for safe prompt submission".to_owned(),
        ));
    }
    let kind = agent
        .agent
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AgentError::Unsupported("neighbor is not a recognized agent".to_owned()))?;
    let session = agent.agent_session.as_ref().ok_or_else(|| {
        AgentError::Unsupported("neighbor has no agent session identity".to_owned())
    })?;
    if session.value.trim().is_empty()
        || session.agent != *kind
        || session.kind.trim().is_empty()
        || session.source.trim().is_empty()
    {
        return Err(AgentError::Malformed(
            "agent session identity is inconsistent".to_owned(),
        ));
    }
    let readiness = readiness(agent.agent_status)?;
    let name = agent
        .name
        .clone()
        .unwrap_or_else(|| format!("{kind} {}", agent.pane_id));
    Ok(AgentTarget {
        direction,
        pane_id: agent.pane_id.clone(),
        workspace_id: agent.workspace_id.clone(),
        tab_id: agent.tab_id.clone(),
        agent_kind: kind.clone(),
        agent_name: name,
        agent_session_id: session.value.clone(),
        readiness,
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect,
        source: source.clone(),
    })
}

fn readiness(value: Option<RawReadiness>) -> Result<AgentReadiness, AgentError> {
    match value {
        Some(RawReadiness::Idle) => Ok(AgentReadiness::Idle),
        Some(RawReadiness::Working) => Ok(AgentReadiness::Working),
        Some(RawReadiness::Done) => Ok(AgentReadiness::Done),
        Some(RawReadiness::Blocked | RawReadiness::Unknown) | None => Err(AgentError::Unsupported(
            "neighbor is not ready for safe prompt submission".to_owned(),
        )),
    }
}

fn unique_agent<'a>(agents: &'a [PaneInfo], pane_id: &str) -> Result<&'a PaneInfo, AgentError> {
    let matches = agents
        .iter()
        .filter(|agent| agent.pane_id == pane_id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [agent] => Ok(agent),
        [] => Err(AgentError::Unsupported(
            "neighbor is not present in the recognized agent list".to_owned(),
        )),
        _ => Err(AgentError::Ambiguous(format!(
            "multiple agents claim pane {pane_id}"
        ))),
    }
}

fn unique_rect(
    layout: &super::contract::PaneLayout,
    pane_id: &str,
) -> Result<PaneRect, AgentError> {
    let matches = layout
        .panes
        .iter()
        .filter(|pane| pane.pane_id == pane_id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [pane] => Ok(pane.rect.into()),
        [] => Err(AgentError::Malformed(format!(
            "pane {pane_id} is absent from layout"
        ))),
        _ => Err(AgentError::Ambiguous(format!(
            "pane {pane_id} has duplicate geometry"
        ))),
    }
}

fn verify_layout_identity(
    pane: &PaneInfo,
    layout: &super::contract::PaneLayout,
) -> Result<(), AgentError> {
    if pane.workspace_id == layout.workspace_id && pane.tab_id == layout.tab_id {
        Ok(())
    } else {
        Err(AgentError::Malformed(
            "current pane and layout identity differ".to_owned(),
        ))
    }
}

fn is_adjacent(source: PaneRect, target: PaneRect, direction: Direction) -> bool {
    let vertical_overlap = source.y < target.bottom() && target.y < source.bottom();
    let horizontal_overlap = source.x < target.right() && target.x < source.right();
    match direction {
        Direction::Left => target.right() == source.x && vertical_overlap,
        Direction::Right => source.right() == target.x && vertical_overlap,
        Direction::Up => target.bottom() == source.y && horizontal_overlap,
        Direction::Down => source.bottom() == target.y && horizontal_overlap,
    }
}

const fn directions() -> [Direction; 4] {
    [
        Direction::Up,
        Direction::Right,
        Direction::Down,
        Direction::Left,
    ]
}

pub(super) const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
}
