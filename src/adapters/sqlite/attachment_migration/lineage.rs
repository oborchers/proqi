//! One-time deterministic synthesis over legacy snapshots, never runtime allocation.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    domain::{AttachmentCounters, ContentAnnotation},
    ports::store::StoreError,
};

#[derive(Default)]
pub(super) struct Lineage {
    snapshots: BTreeMap<(String, String), Vec<usize>>,
    versions: BTreeMap<String, Vec<usize>>,
    current: BTreeMap<String, (String, Vec<usize>)>,
    history: super::history::History,
    scope: String,
    collected: BTreeSet<String>,
    nodes: Vec<Node>,
    counters: AttachmentCounters,
}

struct Node {
    parent: usize,
    signature: String,
    snapshots: BTreeSet<usize>,
    image: bool,
    ordinal: Option<u64>,
}

impl Lineage {
    pub(super) fn register(
        &mut self,
        id: &str,
        content: &str,
        annotations: &Value,
    ) -> Result<Vec<usize>, StoreError> {
        self.register_version(id, content, annotations, false)
    }

    fn register_version(
        &mut self,
        id: &str,
        content: &str,
        annotations: &Value,
        fresh: bool,
    ) -> Result<Vec<usize>, StoreError> {
        let key = snapshot_key(id, content, annotations)?;
        let scoped = (self.scope.clone(), key.clone());
        if let Some(existing) = self.snapshots.get(&scoped) {
            return Ok(existing.clone());
        }
        if !fresh
            && let Some((current_key, existing)) = self.current.get(id)
            && current_key == &key
        {
            let nodes = existing.clone();
            self.snapshots.insert(scoped, nodes.clone());
            return Ok(nodes);
        }
        if !fresh && let Some(existing) = self.versions.get(&key) {
            let nodes = existing.clone();
            self.snapshots.insert(scoped, nodes.clone());
            return Ok(nodes);
        }
        let decoded: Vec<ContentAnnotation> =
            serde_json::from_value(annotations.clone()).map_err(corrupt)?;
        crate::domain::validate_annotations(content, &decoded).map_err(corrupt)?;
        let snapshot = self.snapshots.len();
        let mut nodes = Vec::new();
        for annotation in decoded {
            let crate::domain::ContentAnnotationKind::Attachment {
                image,
                display_name,
                ordinal,
            } = annotation.kind
            else {
                continue;
            };
            if ordinal.is_some() {
                return Err(corrupt("ordinal in legacy schema"));
            }
            let node = self.nodes.len();
            let signature = serde_json::to_string(&(
                image,
                display_name,
                &content[annotation.start..annotation.end],
            ))
            .map_err(corrupt)?;
            self.nodes.push(Node {
                parent: node,
                signature,
                snapshots: BTreeSet::from([snapshot]),
                image,
                ordinal: None,
            });
            nodes.push(node);
        }
        self.snapshots.insert(scoped, nodes.clone());
        self.versions.insert(key, nodes.clone());
        Ok(nodes)
    }

    pub(super) fn collect(&mut self, value: &Value) -> Result<(), StoreError> {
        self.scope = serde_json::to_string(value).map_err(corrupt)?;
        if !self.collected.insert(self.scope.clone()) {
            return Ok(());
        }
        if let Some(forward) = value.get("forward") {
            self.collect_value(forward, true)?;
            if let Some(inverse) = value.get("inverse") {
                self.collect_value(inverse, false)?;
            }
        } else {
            self.collect_value(value, true)?;
        }
        if let Some((payload, forward)) = self.history.observe(value) {
            self.scope = serde_json::to_string(&payload).map_err(corrupt)?;
            self.apply_state(&payload, forward)?;
        }
        Ok(())
    }

    fn apply_state(&mut self, value: &Value, forward: bool) -> Result<(), StoreError> {
        if let Some(mutation) = value.get(if forward { "forward" } else { "inverse" }) {
            return self.apply_state(mutation, forward);
        }
        if let Some(children) = value.get("mutations").and_then(Value::as_array) {
            for child in children {
                self.apply_state(child, forward)?;
            }
            return Ok(());
        }
        if let Some(thought) = value.get("thought") {
            return self.bind_current(thought, "id", "content", "annotations");
        }
        if value.get("before_content").is_some() {
            let prefix = if value.get("mutation").is_some() || forward {
                "after"
            } else {
                "before"
            };
            return self.bind_current(
                value,
                "thought_id",
                &format!("{prefix}_content"),
                &format!("{prefix}_annotations"),
            );
        }
        Ok(())
    }

    fn bind_current(
        &mut self,
        value: &Value,
        id_key: &str,
        content_key: &str,
        annotations_key: &str,
    ) -> Result<(), StoreError> {
        let id = string(value, id_key)?;
        let content = string(value, content_key)?;
        let annotations = &value[annotations_key];
        let key = snapshot_key(id, content, annotations)?;
        let nodes = self.register(id, content, annotations)?;
        self.current.insert(id.to_owned(), (key, nodes));
        Ok(())
    }

    pub(super) fn current_board(&mut self) {
        self.scope.clear();
    }

    fn collect_value(&mut self, value: &Value, forward: bool) -> Result<(), StoreError> {
        match value {
            Value::Object(object) => {
                if let (Some(id), Some(content), Some(annotations)) = (
                    object.get("id").and_then(Value::as_str),
                    object.get("content").and_then(Value::as_str),
                    object.get("annotations"),
                ) {
                    self.register_version(id, content, annotations, forward)?;
                }
                self.collect_prefixed(value, forward)?;
                for child in object.values() {
                    self.collect_value(child, forward)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.collect_value(child, forward)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_prefixed(&mut self, value: &Value, forward: bool) -> Result<(), StoreError> {
        let Some(id) = value.get("thought_id").and_then(Value::as_str) else {
            return Ok(());
        };
        for prefix in ["before", "after", "expected"] {
            if let (Some(content), Some(annotations)) = (
                value
                    .get(format!("{prefix}_content"))
                    .and_then(Value::as_str),
                value.get(format!("{prefix}_annotations")),
            ) {
                self.register_version(id, content, annotations, forward && prefix == "after")?;
            }
        }
        Ok(())
    }

    pub(super) fn link_payload(&mut self, value: &Value) -> Result<(), StoreError> {
        self.scope = serde_json::to_string(value).map_err(corrupt)?;
        if value.get("before_annotations").is_some() {
            let before = self.prefixed(value, "before")?;
            let after = self.prefixed(value, "after")?;
            self.link(&before, &after);
        } else if matches!(
            value.get("kind").and_then(Value::as_str),
            Some("split" | "extract" | "merge")
        ) {
            let forward = value
                .get("forward")
                .ok_or_else(|| corrupt("missing forward mutation"))?;
            let mut sources = Vec::new();
            let mut destinations = Vec::new();
            self.transform_sides(forward, &mut sources, &mut destinations)?;
            sources.sort_by_key(|(position, _)| *position);
            let sources = sources
                .into_iter()
                .flat_map(|(_, nodes)| nodes)
                .collect::<Vec<_>>();
            self.link(&sources, &destinations);
        }
        Ok(())
    }

    fn transform_sides(
        &mut self,
        value: &Value,
        sources: &mut Vec<(u64, Vec<usize>)>,
        destinations: &mut Vec<usize>,
    ) -> Result<(), StoreError> {
        match value.get("mutation").and_then(Value::as_str) {
            Some("batch") => {
                for child in value["mutations"]
                    .as_array()
                    .ok_or_else(|| corrupt("invalid batch"))?
                {
                    self.transform_sides(child, sources, destinations)?;
                }
            }
            Some("replace_content") => {
                sources.push((0, self.prefixed(value, "before")?));
                destinations.extend(self.prefixed(value, "after")?);
            }
            Some("set_deletion_exact") => {
                sources.push((
                    value["expected_position"]
                        .as_u64()
                        .ok_or_else(|| corrupt("invalid position"))?,
                    self.prefixed(value, "expected")?,
                ));
            }
            Some("add_thought") => {
                let thought = &value["thought"];
                destinations.extend(self.register(
                    string(thought, "id")?,
                    string(thought, "content")?,
                    &thought["annotations"],
                )?);
            }
            _ => return Err(corrupt("invalid transformation mutation")),
        }
        Ok(())
    }

    fn prefixed(&mut self, value: &Value, prefix: &str) -> Result<Vec<usize>, StoreError> {
        self.register(
            string(value, "thought_id")?,
            string(value, &format!("{prefix}_content"))?,
            &value[format!("{prefix}_annotations")],
        )
    }

    fn root(&self, mut node: usize) -> usize {
        while self.nodes[node].parent != node {
            node = self.nodes[node].parent;
        }
        node
    }

    fn link(&mut self, sources: &[usize], destinations: &[usize]) {
        let mut used = BTreeSet::new();
        for &source in sources {
            let Some(&destination) = destinations.iter().find(|&&destination| {
                !used.contains(&destination)
                    && self.nodes[source].signature == self.nodes[destination].signature
            }) else {
                continue;
            };
            used.insert(destination);
            let left = self.root(source);
            let right = self.root(destination);
            if left == right
                || !self.nodes[left]
                    .snapshots
                    .is_disjoint(&self.nodes[right].snapshots)
            {
                continue;
            }
            let snapshots = self.nodes[right].snapshots.clone();
            self.nodes[left].snapshots.extend(snapshots);
            self.nodes[right].parent = left;
        }
    }

    pub(super) fn assign(
        &mut self,
        id: &str,
        content: &str,
        annotations: &mut Value,
    ) -> Result<(), StoreError> {
        let nodes = self.register(id, content, annotations)?;
        let mut nodes = nodes.into_iter();
        for annotation in annotations
            .as_array_mut()
            .ok_or_else(|| corrupt("invalid annotation array"))?
        {
            if annotation["kind"]["kind"] != "attachment" {
                continue;
            }
            let root = self.root(nodes.next().ok_or_else(|| corrupt("missing occurrence"))?);
            let ordinal = self.ordinal(root)?;
            annotation["kind"]["ordinal"] = Value::from(ordinal);
        }
        Ok(())
    }

    fn ordinal(&mut self, root: usize) -> Result<u64, StoreError> {
        if let Some(ordinal) = self.nodes[root].ordinal {
            return Ok(ordinal);
        }
        let image = self.nodes[root].image;
        let next = if image {
            self.counters.image()
        } else {
            self.counters.file()
        }
        .checked_add(1)
        .ok_or_else(|| corrupt("ordinal overflow"))?;
        self.counters = AttachmentCounters::new(
            if image { next } else { self.counters.image() },
            if image { self.counters.file() } else { next },
        )
        .map_err(corrupt)?;
        self.nodes[root].ordinal = Some(next);
        Ok(next)
    }

    fn assign_prefixed(&mut self, value: &mut Value) -> Result<(), StoreError> {
        let Some(id) = value
            .get("thought_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return Ok(());
        };
        for prefix in ["before", "after", "expected"] {
            if let Some(content) = value
                .get(format!("{prefix}_content"))
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                self.assign(&id, &content, &mut value[format!("{prefix}_annotations")])?;
            }
        }
        Ok(())
    }

    pub(super) fn assign_payload(&mut self, value: &mut Value) -> Result<(), StoreError> {
        self.scope = serde_json::to_string(value).map_err(corrupt)?;
        self.assign_value(value)
    }

    fn assign_value(&mut self, value: &mut Value) -> Result<(), StoreError> {
        if value.get("annotations").is_some()
            && value.get("content").is_some()
            && value.get("id").is_some()
        {
            let id = string(value, "id")?.to_owned();
            let content = string(value, "content")?.to_owned();
            self.assign(&id, &content, &mut value["annotations"])?;
            return Ok(());
        }
        self.assign_prefixed(value)?;
        match value {
            Value::Object(object) => {
                for child in object.values_mut() {
                    self.assign_value(child)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    self.assign_value(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) const fn counters(&self) -> AttachmentCounters {
        self.counters
    }
}

fn snapshot_key(id: &str, content: &str, annotations: &Value) -> Result<String, StoreError> {
    serde_json::to_string(&(id, content, annotations)).map_err(corrupt)
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| corrupt(format!("missing {key}")))
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
}
