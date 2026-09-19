//! Failure-only, bounded, allowlisted evidence from synthetic migration cohorts.

use proqi::{
    domain::{InstanceId, RequestId, SessionId},
    ports::runtime::InstanceInfo,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::Instant,
};

use super::gateway_trace::Event;

const INPUT_LIMIT: u64 = 256 * 1024;

pub(super) struct Evidence {
    state: PathBuf,
    count: usize,
    started: Instant,
    stage: Cell<&'static str>,
    execution: RefCell<Option<Value>>,
}

impl Evidence {
    pub(super) fn new(state: &Path, count: usize) -> Self {
        Self {
            state: state.to_path_buf(),
            count,
            started: Instant::now(),
            stage: Cell::new("owner_startup"),
            execution: RefCell::new(None),
        }
    }

    pub(super) fn stage(&self, stage: &'static str) {
        self.stage.set(stage);
    }

    pub(super) fn execution(&self, value: &Value) {
        *self.execution.borrow_mut() = Some(sanitize_execution(value));
    }

    pub(super) fn summary(&self) -> Value {
        let execution = self.execution.borrow().clone().or_else(|| {
            serde_json::from_str::<Value>(&bounded_text(
                &self.state.join("coordinator-execution.json"),
            ))
            .ok()
            .map(|value| sanitize_execution(&value))
        });
        let trace = bounded_text(&self.state.join("gateway-trace.jsonl"));
        let events = trace
            .lines()
            .take(512)
            .filter_map(|line| serde_json::from_str::<Event>(line).ok())
            .collect::<Vec<_>>();
        json!({"schema_version": 1, "cohort_size": self.count,
            "stage": self.stage.get(), "elapsed_ms": self.started.elapsed().as_millis(),
            "execution": execution, "gateway_events": events,
            "runtime": runtime_summary(&self.state),
            "diagnostic_counts": diagnostic_counts(&self.state),
            "driver_exit": driver_exit(&self.state),
        })
    }
}

impl Drop for Evidence {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        let report = self.summary();
        eprintln!("migration failure evidence: {report}");
        // Fixed artifact root; never retain the fixture state or terminal output.
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/qualification-evidence");
        let _retained = persist(&directory, &report);
    }
}

fn persist(directory: &Path, report: &Value) -> std::io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let mut file = tempfile::Builder::new()
        .prefix("migration-")
        .suffix(".json")
        .tempfile_in(directory)?;
    serde_json::to_writer_pretty(file.as_file_mut(), report)?;
    file.flush()?;
    file.keep()
        .map(|(_, path)| path)
        .map_err(|error| error.error)
}

#[path = "evidence_tests.rs"]
mod tests;

fn bounded_text(path: &Path) -> String {
    let mut content = String::new();
    if let Ok(file) = File::open(path) {
        let _read = file.take(INPUT_LIMIT).read_to_string(&mut content);
    }
    content
}

fn runtime_summary(state: &Path) -> Vec<Value> {
    let Ok(entries) = fs::read_dir(state.join("runtime/instances")) else {
        return Vec::new();
    };
    let mut records = entries.take(64).filter_map(Result::ok)
        .filter_map(|entry| serde_json::from_str::<InstanceInfo>(&bounded_text(&entry.path())).ok())
        .map(|owner| {
            let present = i32::try_from(owner.pid).ok().and_then(rustix::process::Pid::from_raw)
                .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok());
            json!({"instance": owner.instance_id, "session": owner.session_id,
                "pid": owner.pid, "process_present": present,
                "control_protocol": owner.control_protocol, "storage_protocol": owner.storage_protocol,
                "endpoint_present": owner.control_endpoint.as_deref().is_some_and(|path| Path::new(path).exists())})
        }).collect::<Vec<_>>();
    records.sort_by_cached_key(Value::to_string);
    records
}

fn diagnostic_counts(state: &Path) -> Value {
    let mut counts = serde_json::Map::new();
    let Ok(entries) = fs::read_dir(state.join("data/diagnostics")) else {
        return Value::Object(counts);
    };
    for entry in entries.take(160).filter_map(Result::ok) {
        for line in bounded_text(&entry.path()).lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let stages = [
                "migration_required",
                "migration_started",
                "migration_completed",
                "follower_revalidated",
                "board_ready",
            ];
            let Some(phase) = value
                .get("stage")
                .and_then(Value::as_str)
                .filter(|stage| stages.contains(stage))
            else {
                continue;
            };
            let count = counts.get(phase).and_then(Value::as_u64).unwrap_or(0) + 1;
            counts.insert(phase.to_owned(), json!(count));
        }
    }
    Value::Object(counts)
}

fn driver_exit(state: &Path) -> Option<i32> {
    bounded_text(&state.join("cohort.exit")).trim().parse().ok()
}

fn sanitize_execution(value: &Value) -> Value {
    let mut output = serde_json::Map::new();
    for key in [
        "selected_participants",
        "prepared_participants",
        "restart_requests",
        "quiescence_requests",
        "quiesced_participants",
        "restart_accepted",
        "replacement_ready",
        "replacement_missing",
    ] {
        output.insert(
            key.to_owned(),
            value
                .get(key)
                .and_then(Value::as_u64)
                .map_or(Value::Null, |number| json!(number)),
        );
    }
    for key in ["quiescence_failed", "restart_failed"] {
        let ids = value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(64)
            .filter_map(Value::as_str)
            .filter_map(|id| id.parse::<InstanceId>().ok())
            .collect::<Vec<_>>();
        output.insert(key.to_owned(), json!(ids));
    }
    let sessions = value
        .get("resumable_sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(64)
        .filter_map(Value::as_str)
        .filter_map(|id| id.parse::<SessionId>().ok())
        .collect::<Vec<_>>();
    output.insert("resumable_sessions".to_owned(), json!(sessions));
    output.insert(
        "operation_id".to_owned(),
        json!(
            value
                .get("operation_id")
                .and_then(Value::as_str)
                .and_then(|id| id.parse::<RequestId>().ok())
        ),
    );
    output.insert(
        "convergence_state_recorded".to_owned(),
        json!(
            value
                .get("convergence_state_recorded")
                .and_then(Value::as_bool)
        ),
    );
    output.insert(
        "status".to_owned(),
        json!(
            value
                .get("status")
                .and_then(|status| status.get("status"))
                .and_then(Value::as_str)
                .filter(|status| matches!(
                    *status,
                    "already_in_progress" | "aborted" | "installed"
                ))
        ),
    );
    Value::Object(output)
}
