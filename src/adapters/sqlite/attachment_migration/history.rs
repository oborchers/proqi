//! Retained receipt chronology distinguishes active state from dormant redo snapshots.
use crate::domain::UndoScope;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct History {
    board: Branch,
    editors: BTreeMap<String, Branch>,
}

#[derive(Default)]
struct Branch {
    entries: Vec<Value>,
    cursor: usize,
}

impl Branch {
    fn push(&mut self, value: &Value) {
        self.entries.truncate(self.cursor);
        self.entries.push(value.clone());
        self.cursor = self.entries.len();
    }

    fn step(&mut self, undo: bool) -> Option<Value> {
        let index = if undo {
            self.cursor.checked_sub(1)?
        } else {
            self.cursor
        };
        let value = self.entries.get(index)?.clone();
        self.cursor = if undo { index } else { index + 1 };
        Some(value)
    }
}

impl History {
    pub(super) fn observe(&mut self, value: &Value) -> Option<(Value, bool)> {
        if value.get("forward").is_some() {
            self.board.push(value);
            return Some((value.clone(), true));
        }
        if value.get("before_content").is_some() {
            let id = value.get("thought_id")?.as_str()?;
            self.editors.entry(id.to_owned()).or_default().push(value);
            return Some((value.clone(), true));
        }
        let receipt = value.as_array()?;
        let scope: UndoScope = serde_json::from_value(receipt.get(1)?.clone()).ok()?;
        let undo = receipt.get(2)?.as_bool()?;
        let branch = match scope {
            UndoScope::Board => &mut self.board,
            UndoScope::Editor { thought_id } => self.editors.get_mut(&thought_id.to_string())?,
        };
        branch.step(undo).map(|payload| (payload, !undo))
    }
}
