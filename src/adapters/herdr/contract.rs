//! Minimal Herdr protocol 19 response shapes consumed by Proqi.

use serde::Deserialize;

use crate::{domain::Direction, ports::agent::PaneRect};

#[derive(Deserialize)]
pub(super) struct Envelope<T> {
    pub(super) result: T,
}

#[derive(Deserialize)]
pub(super) struct ErrorEnvelope {
    pub(super) error: ErrorBody,
}

#[derive(Deserialize)]
pub(super) struct ErrorBody {
    pub(super) code: String,
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct SchemaDocument {
    pub(super) protocol: u32,
    pub(super) schema_version: u32,
}

#[derive(Deserialize)]
pub(super) struct SnapshotBody {
    pub(super) snapshot: Snapshot,
}

#[derive(Deserialize)]
pub(super) struct Snapshot {
    pub(super) protocol: u32,
    pub(super) version: String,
}

#[derive(Deserialize)]
pub(super) struct CurrentBody {
    pub(super) pane: PaneInfo,
}

#[derive(Clone, Deserialize)]
pub(super) struct PaneInfo {
    pub(super) pane_id: String,
    pub(super) workspace_id: String,
    pub(super) tab_id: String,
    pub(super) agent: Option<String>,
    pub(super) name: Option<String>,
    pub(super) agent_status: Option<RawReadiness>,
    pub(super) agent_session: Option<AgentSession>,
}

#[derive(Clone, Deserialize)]
pub(super) struct AgentSession {
    pub(super) agent: String,
    pub(super) kind: String,
    pub(super) source: String,
    pub(super) value: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RawReadiness {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

#[derive(Deserialize)]
pub(super) struct LayoutBody {
    pub(super) layout: PaneLayout,
}

#[derive(Clone, Deserialize)]
pub(super) struct PaneLayout {
    pub(super) workspace_id: String,
    pub(super) tab_id: String,
    pub(super) panes: Vec<LayoutPane>,
}

#[derive(Clone, Deserialize)]
pub(super) struct LayoutPane {
    pub(super) pane_id: String,
    pub(super) rect: PaneRectWire,
}

#[derive(Clone, Copy, Deserialize)]
pub(super) struct PaneRectWire {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
}

impl From<PaneRectWire> for PaneRect {
    fn from(value: PaneRectWire) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AgentsBody {
    pub(super) agents: Vec<PaneInfo>,
}

#[derive(Deserialize)]
pub(super) struct NeighborBody {
    pub(super) neighbor: Neighbor,
}

#[derive(Deserialize)]
pub(super) struct Neighbor {
    pub(super) pane_id: String,
    pub(super) direction: Direction,
    #[serde(rename = "neighbor_pane_id")]
    pub(super) candidate_pane_id: Option<String>,
    pub(super) layout: PaneLayout,
}

#[derive(Deserialize)]
pub(super) struct PromptBody {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) agent: PaneInfo,
}
