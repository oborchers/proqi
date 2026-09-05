//! Offline classification of one downloaded Herdr schema through the product adapter.

use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File},
    io::Read as _,
    path::Path,
};

use proqi::{
    adapters::{
        herdr::{HerdrCompatibilityPolicy, HerdrGateway},
        process::MAX_CAPTURE_BYTES,
    },
    ports::{
        agent::{AgentError, AgentGateway as _},
        environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner},
    },
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const MAX_SCHEMA_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn run(
    root: &Path,
    schema_path: &Path,
    stderr_path: Option<&Path>,
) -> Result<(), String> {
    let report = inspect(root, schema_path, stderr_path)?;
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|error| format!("serialize report: {error}"))?
    );
    Ok(())
}

pub(crate) fn policy_findings(root: &Path) -> Result<Vec<String>, String> {
    let first = HerdrCompatibilityPolicy::qualified_from();
    let last = HerdrCompatibilityPolicy::qualified_through();
    let provisional = HerdrCompatibilityPolicy::provisional_protocol();
    let fixture_bounds = qualified_bounds(root)?;
    let mut found = Vec::new();
    if fixture_bounds != (first, last) {
        found.push(format!(
            "Herdr compatibility policy qualifies {first} through {last}, but fixtures cover {} through {}",
            fixture_bounds.0, fixture_bounds.1
        ));
    }
    for protocol in first..=last {
        let schema = qualified_fixture(root, protocol)?;
        if gateway_result(schema, protocol).is_err() {
            found.push(format!(
                "qualified Herdr protocol fixture {protocol} is rejected by the adapter policy"
            ));
        }
    }
    let future = provisional
        .checked_add(1)
        .ok_or_else(|| "provisional protocol overflow".to_owned())?;
    let latest = qualified_fixture(root, last)?;
    if gateway_result(with_protocol(&latest, provisional)?, provisional).is_err() {
        found.push(format!(
            "Herdr protocol {provisional} must be the accepted provisional version"
        ));
    }
    if gateway_result(with_protocol(&latest, future)?, future).is_ok() {
        found.push(format!(
            "Herdr protocol {future} exceeds the one-version provisional window but is accepted"
        ));
    }
    Ok(found)
}

fn inspect(root: &Path, schema_path: &Path, stderr_path: Option<&Path>) -> Result<Value, String> {
    let raw = read_bounded(schema_path, MAX_SCHEMA_BYTES)?;
    let capture_oversized = raw.len() > MAX_SCHEMA_BYTES;
    let production_limit = usize::try_from(MAX_CAPTURE_BYTES)
        .map_err(|error| format!("convert production output limit: {error}"))?;
    let production_oversized = raw.len() > production_limit;
    let stderr_oversized = stderr_path
        .map(|path| read_bounded(path, production_limit))
        .transpose()?
        .is_some_and(|stderr| stderr.len() > production_limit);
    let qualified_from = HerdrCompatibilityPolicy::qualified_from();
    let qualified_through = HerdrCompatibilityPolicy::qualified_through();
    let policy_drift = !policy_findings(root)?.is_empty();
    let provisional_protocol = HerdrCompatibilityPolicy::provisional_protocol();
    let digest = digest_hex(&raw);
    let parsed = (!capture_oversized && !production_oversized)
        .then(|| serde_json::from_slice::<Value>(&raw));
    let protocol = parsed
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .and_then(|value| exact_u32(value.get("protocol")));
    let schema_version = parsed
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .and_then(|value| exact_u32(value.get("schema_version")));

    let classification = match (
        policy_drift,
        capture_oversized,
        production_oversized || stderr_oversized,
        parsed,
        protocol,
    ) {
        (true, _, _, _, _) => ("policy_drift", false, Some("policy_drift")),
        (false, _, true, _, _) => ("incompatible", false, Some("output_limit")),
        (false, true, false, _, _) => ("incompatible", false, Some("too_large")),
        (false, false, false, Some(Ok(_)), Some(protocol)) => classify_gateway(
            raw,
            protocol,
            qualified_from,
            qualified_through,
            provisional_protocol,
        ),
        (false, false, false, _, _) => ("incompatible", false, Some("malformed")),
    };
    let (compatibility, compatible, reason_code) = classification;
    Ok(json!({
        "schema_version": schema_version,
        "protocol": protocol,
        "qualified_from": qualified_from,
        "qualified_through": qualified_through,
        "provisional_protocol": provisional_protocol,
        "compatibility": compatibility,
        "compatible": compatible,
        "reason_code": reason_code,
        "issue_required": compatibility != "qualified",
        "schema_sha256": digest,
    }))
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let limit = u64::try_from(maximum)
        .map_err(|error| format!("convert schema limit: {error}"))?
        .saturating_add(1);
    let mut raw = Vec::new();
    file.take(limit)
        .read_to_end(&mut raw)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(raw)
}

fn classify_gateway(
    schema: Vec<u8>,
    protocol: u32,
    qualified_from: u32,
    qualified_through: u32,
    provisional_protocol: u32,
) -> (&'static str, bool, Option<&'static str>) {
    match gateway_result(schema, protocol) {
        Ok(()) if (qualified_from..=qualified_through).contains(&protocol) => {
            ("qualified", true, None)
        }
        Ok(()) if protocol == provisional_protocol => {
            ("provisional", true, Some("qualify_provisional"))
        }
        Ok(()) => ("policy_drift", false, Some("policy_drift")),
        Err(error) => ("incompatible", false, Some(error.stable_code().as_str())),
    }
}

fn gateway_result(schema: Vec<u8>, protocol: u32) -> Result<(), AgentError> {
    let runner = ProbeRunner { schema, protocol };
    let mut gateway = HerdrGateway::new(OsString::from("herdr-schema-probe"), runner, true);
    gateway.capabilities().map(|_| ())
}

fn qualified_bounds(root: &Path) -> Result<(u32, u32), String> {
    let directory = root.join("src/adapters/herdr/tests/fixtures");
    let mut protocols = BTreeSet::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read fixture entry: {error}"))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(protocol) = name
            .strip_prefix("protocol")
            .and_then(|suffix| suffix.parse::<u32>().ok())
        else {
            continue;
        };
        let schema_path = entry.path().join("schema.json");
        let schema = qualified_fixture(root, protocol)?;
        let value: Value = serde_json::from_slice(&schema)
            .map_err(|error| format!("parse {}: {error}", schema_path.display()))?;
        if exact_u32(value.get("protocol")) != Some(protocol) {
            return Err(format!(
                "{} protocol does not match its fixture directory",
                schema_path.display()
            ));
        }
        protocols.insert(protocol);
    }
    let Some(first) = protocols.first().copied() else {
        return Err("no qualified Herdr protocol fixtures found".to_owned());
    };
    let Some(last) = protocols.last().copied() else {
        return Err("no qualified Herdr protocol fixtures found".to_owned());
    };
    if (first..=last).any(|protocol| !protocols.contains(&protocol)) {
        return Err("qualified Herdr protocol fixtures are not contiguous".to_owned());
    }
    Ok((first, last))
}

fn qualified_fixture(root: &Path, protocol: u32) -> Result<Vec<u8>, String> {
    let path = root
        .join("src/adapters/herdr/tests/fixtures")
        .join(format!("protocol{protocol}/schema.json"));
    fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn with_protocol(schema: &[u8], protocol: u32) -> Result<Vec<u8>, String> {
    let mut value: Value = serde_json::from_slice(schema)
        .map_err(|error| format!("parse latest qualified schema: {error}"))?;
    value["protocol"] = json!(protocol);
    serde_json::to_vec(&value).map_err(|error| format!("serialize projected schema: {error}"))
}

fn exact_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

struct ProbeRunner {
    schema: Vec<u8>,
    protocol: u32,
}

impl ProcessRunner for ProbeRunner {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        let args = request
            .args
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>();
        let stdout = match args.as_slice() {
            [api, schema, json] if api == "api" && schema == "schema" && json == "--json" => {
                self.schema.clone()
            }
            [api, snapshot] if api == "api" && snapshot == "snapshot" => serde_json::to_vec(
                &json!({"result":{"snapshot":{"protocol":self.protocol,"version":"schema-probe"}}}),
            )
            .map_err(|error| ProcessError::Io(error.to_string()))?,
            [pane, current, flag] if pane == "pane" && current == "current" && flag == "--current" => {
                serde_json::to_vec(&json!({"result":{"pane":{"pane_id":"probe:p1","workspace_id":"probe","tab_id":"probe:t1"}}}))
                    .map_err(|error| ProcessError::Io(error.to_string()))?
            }
            [pane, layout, flag, target]
                if pane == "pane"
                    && layout == "layout"
                    && flag == "--pane"
                    && target == "probe:p1" =>
            {
                serde_json::to_vec(&json!({"result":{"layout":{"workspace_id":"probe","tab_id":"probe:t1","panes":[{"pane_id":"probe:p1","rect":{"x":0,"y":0,"width":80,"height":24}}]}}}))
                    .map_err(|error| ProcessError::Io(error.to_string()))?
            }
            _ => return Err(ProcessError::Io(format!("unexpected probe command: {args:?}"))),
        };
        Ok(ProcessOutput {
            exit_code: Some(0),
            stdout,
            stderr: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{inspect, policy_findings};
    use serde_json::{Value, json};

    #[test]
    fn classifies_qualified_provisional_and_unsupported_protocols() {
        let temporary = tempfile::tempdir().expect("temporary root");
        seed_fixtures(temporary.path());
        for (protocol, compatibility, compatible, reason) in [
            (19, "qualified", true, Value::Null),
            (20, "qualified", true, Value::Null),
            (21, "provisional", true, json!("qualify_provisional")),
            (22, "incompatible", false, json!("unsupported")),
        ] {
            let path = temporary.path().join(format!("schema-{protocol}.json"));
            write_schema(&path, protocol);
            let report = inspect(temporary.path(), &path, None).expect("inspect schema");
            assert_eq!(report["compatibility"], compatibility);
            assert_eq!(report["compatible"], compatible);
            assert_eq!(report["reason_code"], reason);
            assert_eq!(report["issue_required"], protocol > 20);
        }
    }

    #[test]
    fn malformed_schema_is_a_content_free_issue_result() {
        let temporary = tempfile::tempdir().expect("temporary root");
        seed_fixtures(temporary.path());
        let path = temporary.path().join("schema.json");
        for contents in [&b""[..], &b"not-json"[..]] {
            std::fs::write(&path, contents).expect("malformed schema");
            let report = inspect(temporary.path(), &path, None).expect("inspect malformed schema");
            assert_eq!(report["protocol"], Value::Null);
            assert_eq!(report["compatibility"], "incompatible");
            assert_eq!(report["reason_code"], "malformed");
            assert_eq!(report["issue_required"], true);
        }
    }

    #[test]
    fn oversized_schema_is_bounded_and_becomes_an_issue_result() {
        let temporary = tempfile::tempdir().expect("temporary root");
        seed_fixtures(temporary.path());
        let path = temporary.path().join("schema.json");
        let file = std::fs::File::create(&path).expect("oversized schema");
        file.set_len((super::MAX_SCHEMA_BYTES + 2) as u64)
            .expect("oversized length");
        let report = inspect(temporary.path(), &path, None).expect("inspect oversized schema");
        assert_eq!(report["protocol"], Value::Null);
        assert_eq!(report["compatibility"], "incompatible");
        assert_eq!(report["reason_code"], "output_limit");
        assert_eq!(report["issue_required"], true);
    }

    #[test]
    fn production_process_output_limit_is_the_compatibility_boundary() {
        let temporary = tempfile::tempdir().expect("temporary root");
        seed_fixtures(temporary.path());
        let path = temporary.path().join("schema.json");
        let limit = usize::try_from(proqi::adapters::process::MAX_CAPTURE_BYTES)
            .expect("production output limit");

        write_padded_schema(&path, 20, limit);
        let accepted = inspect(temporary.path(), &path, None).expect("inspect boundary schema");
        assert_eq!(accepted["compatibility"], "qualified");
        assert_eq!(accepted["issue_required"], false);

        write_padded_schema(&path, 20, limit + 1);
        let rejected = inspect(temporary.path(), &path, None).expect("inspect oversized schema");
        assert_eq!(rejected["protocol"], Value::Null);
        assert_eq!(rejected["compatibility"], "incompatible");
        assert_eq!(rejected["reason_code"], "output_limit");
        assert_eq!(rejected["issue_required"], true);

        write_padded_schema(&path, 20, limit);
        let stderr = temporary.path().join("stderr");
        std::fs::write(&stderr, vec![b'x'; limit]).expect("boundary stderr");
        let accepted =
            inspect(temporary.path(), &path, Some(&stderr)).expect("inspect boundary stderr");
        assert_eq!(accepted["compatibility"], "qualified");
        assert_eq!(accepted["issue_required"], false);

        std::fs::write(&stderr, vec![b'x'; limit + 1]).expect("oversized stderr");
        let rejected =
            inspect(temporary.path(), &path, Some(&stderr)).expect("inspect oversized stderr");
        assert_eq!(rejected["protocol"], 20);
        assert_eq!(rejected["compatibility"], "incompatible");
        assert_eq!(rejected["reason_code"], "output_limit");
        assert_eq!(rejected["issue_required"], true);
    }

    #[test]
    fn repository_policy_enforces_exactly_one_provisional_protocol() {
        let temporary = tempfile::tempdir().expect("temporary root");
        seed_fixtures(temporary.path());
        assert!(
            policy_findings(temporary.path())
                .expect("policy findings")
                .is_empty()
        );
        write_fixture(temporary.path(), 21);
        let found = policy_findings(temporary.path()).expect("drift findings");
        assert!(
            found
                .iter()
                .any(|item| item.contains("fixtures cover 19 through 21"))
        );
    }

    fn seed_fixtures(root: &std::path::Path) {
        for protocol in [19, 20] {
            write_fixture(root, protocol);
        }
    }

    fn write_fixture(root: &std::path::Path, protocol: u32) {
        let directory = root
            .join("src/adapters/herdr/tests/fixtures")
            .join(format!("protocol{protocol}"));
        std::fs::create_dir_all(&directory).expect("fixture directory");
        write_schema(&directory.join("schema.json"), protocol);
    }

    fn write_schema(path: &std::path::Path, protocol: u32) {
        let mut schema: Value = serde_json::from_str(include_str!(
            "../../src/adapters/herdr/tests/fixtures/protocol20/schema.json"
        ))
        .expect("recorded schema");
        schema["protocol"] = json!(protocol);
        std::fs::write(path, serde_json::to_vec(&schema).expect("schema bytes"))
            .expect("write schema");
    }

    fn write_padded_schema(path: &std::path::Path, protocol: u32, length: usize) {
        let mut schema: Value = serde_json::from_str(include_str!(
            "../../src/adapters/herdr/tests/fixtures/protocol20/schema.json"
        ))
        .expect("recorded schema");
        schema["protocol"] = json!(protocol);
        let mut bytes = serde_json::to_vec(&schema).expect("schema bytes");
        assert!(bytes.len() <= length);
        bytes.resize(length, b' ');
        std::fs::write(path, bytes).expect("write padded schema");
    }
}
