//! Repository policy for the root Herdr plugin manifest.
//!
//! Cargo.toml owns the version. The Herdr adapter owns the plugin, action, pane,
//! launcher, and install identities, and the application owns the pane label.
//! The manifest must restate exactly those values so the marketplace listing,
//! the toggle, and the installed binary cannot drift apart.

use std::path::Path;

use proqi::{
    adapters::herdr::{
        HerdrCompatibilityPolicy, INSTALL_PATH, LAUNCHER_PATH, MIN_HERDR_VERSION,
        PANE_ENTRYPOINT_ID, PLUGIN_ID, SESSION_ENVIRONMENT, TOGGLE_ACTION_ID, TOGGLE_CAPABILITY,
    },
    application::COMPANION_PANE_LABEL,
};
use toml::{Table, Value};

const MANIFEST: &str = "herdr-plugin.toml";
const MAX_MANIFEST_BYTES: usize = 16 * 1024;
/// Herdr 0.8.0 introduced protocol 19, the oldest protocol Proqi qualifies.
const MIN_HERDR_PROTOCOL: u32 = 19;
const TOP_LEVEL: &[&str] = &[
    "id",
    "name",
    "version",
    "min_herdr_version",
    "description",
    "platforms",
    "build",
    "actions",
    "panes",
];

pub(crate) fn findings(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join(MANIFEST);
    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let version = super::release::workspace_version(root)?.to_string();
    let mut found = manifest_findings(&contents, &version, |relative| {
        root.join(relative).is_file()
    });
    let launcher = std::fs::read_to_string(root.join(LAUNCHER_PATH)).unwrap_or_default();
    found.extend(launcher_findings(&launcher));
    Ok(found)
}

/// The launcher restates Rust-owned tokens in shell syntax; each must match its owner.
fn launcher_findings(launcher: &str) -> Vec<String> {
    [
        format!("*'\"{TOGGLE_CAPABILITY}\":true'*)"),
        "exec \"$proqi\" herdr toggle".to_owned(),
        format!("exec \"$proqi\" --resume \"${SESSION_ENVIRONMENT}\""),
    ]
    .into_iter()
    .filter(|token| !launcher.contains(token.as_str()))
    .map(|token| format!("{LAUNCHER_PATH} must contain `{token}`"))
    .collect()
}

fn manifest_findings(contents: &str, version: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    if contents.len() > MAX_MANIFEST_BYTES {
        found.push(format!("{MANIFEST} exceeds {MAX_MANIFEST_BYTES} bytes"));
    }
    if HerdrCompatibilityPolicy::qualified_from() != MIN_HERDR_PROTOCOL {
        found.push(format!(
            "Herdr qualification starts at protocol {}, so {MANIFEST} min_herdr_version {MIN_HERDR_VERSION} needs review",
            HerdrCompatibilityPolicy::qualified_from()
        ));
    }
    let manifest: Value = match toml::from_str(contents) {
        Ok(manifest) => manifest,
        Err(error) => {
            found.push(format!("{MANIFEST} is not valid TOML: {error}"));
            return found;
        }
    };
    let Some(table) = manifest.as_table() else {
        found.push(format!("{MANIFEST} must be a TOML table"));
        return found;
    };
    identity_findings(&mut found, table, version);
    entrypoint_findings(&mut found, table);
    for script in [LAUNCHER_PATH, INSTALL_PATH] {
        if !exists(script) {
            found.push(format!("{MANIFEST} references missing {script}"));
        }
    }
    found
}

/// Package metadata owned by Cargo, the Herdr adapter, and the marketplace.
fn identity_findings(found: &mut Vec<String>, table: &Table, version: &str) {
    for key in table
        .keys()
        .filter(|key| !TOP_LEVEL.contains(&key.as_str()))
    {
        found.push(format!(
            "{MANIFEST} declares unreviewed top-level key `{key}`; startup hooks, events, and link handlers need an explicit policy change"
        ));
    }
    for (key, expected) in [
        ("id", PLUGIN_ID),
        ("name", "Proqi"),
        ("version", version),
        ("min_herdr_version", MIN_HERDR_VERSION),
    ] {
        expect_string(found, table.get(key), key, expected);
    }
    if table
        .get("description")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        found.push(format!(
            "{MANIFEST} needs a nonempty description for the marketplace"
        ));
    }
    expect_strings(
        found,
        table.get("platforms"),
        "platforms",
        &["linux", "macos"],
    );
}

/// The build step, toggle action, and pane entrypoint the adapter depends on.
fn entrypoint_findings(found: &mut Vec<String>, table: &Table) {
    if let [build] = entries(found, table, "build", 1).as_slice() {
        expect_strings(
            found,
            build.get("command"),
            "build command",
            &["sh", INSTALL_PATH],
        );
    }
    if let [action] = entries(found, table, "actions", 1).as_slice() {
        expect_string(found, action.get("id"), "action id", TOGGLE_ACTION_ID);
        let command = ["sh", LAUNCHER_PATH, "toggle"];
        expect_strings(found, action.get("command"), "action command", &command);
    }
    if let [pane] = entries(found, table, "panes", 1).as_slice() {
        expect_string(found, pane.get("id"), "pane id", PANE_ENTRYPOINT_ID);
        expect_string(found, pane.get("title"), "pane title", COMPANION_PANE_LABEL);
        expect_string(found, pane.get("placement"), "pane placement", "split");
        let command = format!("exec sh \"$HERDR_PLUGIN_ROOT/{LAUNCHER_PATH}\" board");
        expect_strings(
            found,
            pane.get("command"),
            "pane command",
            &["sh", "-c", &command],
        );
    }
}

fn entries<'a>(
    found: &mut Vec<String>,
    table: &'a Table,
    key: &str,
    count: usize,
) -> Vec<&'a Table> {
    let entries: Vec<_> = table
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(Value::as_table).collect())
        .unwrap_or_default();
    if entries.len() != count {
        found.push(format!(
            "{MANIFEST} must declare exactly {count} [[{key}]] entry"
        ));
    }
    entries
}

fn expect_string(found: &mut Vec<String>, value: Option<&Value>, key: &str, expected: &str) {
    if value.and_then(Value::as_str) != Some(expected) {
        found.push(format!("{MANIFEST} {key} must be `{expected}`"));
    }
}

fn expect_strings(found: &mut Vec<String>, value: Option<&Value>, key: &str, expected: &[&str]) {
    let actual: Option<Vec<&str>> = value
        .and_then(Value::as_array)
        .and_then(|values| values.iter().map(Value::as_str).collect());
    if actual.as_deref() != Some(expected) {
        found.push(format!("{MANIFEST} {key} must be {expected:?}"));
    }
}

#[cfg(test)]
mod tests {
    use super::{launcher_findings, manifest_findings};

    const REPOSITORY_MANIFEST: &str = include_str!("../../herdr-plugin.toml");

    fn version() -> String {
        let cargo: toml::Value =
            toml::from_str(include_str!("../../Cargo.toml")).expect("Cargo manifest");
        cargo["workspace"]["package"]["version"]
            .as_str()
            .expect("version")
            .to_owned()
    }

    #[test]
    fn repository_manifest_is_accepted() {
        assert_eq!(
            manifest_findings(REPOSITORY_MANIFEST, &version(), |_| true),
            Vec::<String>::new()
        );
    }

    #[test]
    fn drift_from_every_owner_is_rejected_with_an_actionable_reason() {
        let cases = [
            ("version = \"", "version = \"9.", "version must be"),
            ("id = \"proqi\"", "id = \"other\"", "id must be `proqi`"),
            (
                "min_herdr_version = \"0.8.0\"",
                "min_herdr_version = \"0.7.0\"",
                "min_herdr_version",
            ),
            ("title = \"Proqi\"", "title = \"Notes\"", "pane title"),
            ("\"toggle\"]", "\"open\"]", "action command"),
            (
                "[\"linux\", \"macos\"]",
                "[\"linux\", \"macos\", \"windows\"]",
                "platforms",
            ),
            ("herdr-plugin/install.sh\"]", "curl.sh\"]", "build command"),
        ];
        for (from, to, reason) in cases {
            let changed = REPOSITORY_MANIFEST.replacen(from, to, 1);
            assert_ne!(changed, REPOSITORY_MANIFEST, "{from}");
            let found = manifest_findings(&changed, &version(), |_| true);
            assert!(
                found.iter().any(|finding| finding.contains(reason)),
                "{reason}: {found:?}"
            );
        }
    }

    #[test]
    fn launcher_tokens_match_their_rust_owners() {
        const LAUNCHER: &str = include_str!("../../herdr-plugin/proqi.sh");
        assert_eq!(launcher_findings(LAUNCHER), Vec::<String>::new());
        let drifted = LAUNCHER.replace("PROQI_HERDR_SESSION", "PROQI_SESSION");
        let found = launcher_findings(&drifted);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("--resume"));
        assert_eq!(launcher_findings("").len(), 3);
    }

    #[test]
    fn unreviewed_hooks_missing_scripts_and_invalid_toml_are_rejected() {
        let hooked = format!("{REPOSITORY_MANIFEST}\n[[startup]]\ncommand = [\"sh\", \"x\"]\n");
        assert!(
            manifest_findings(&hooked, &version(), |_| true)
                .iter()
                .any(|finding| finding.contains("`startup`"))
        );
        assert!(
            manifest_findings(REPOSITORY_MANIFEST, &version(), |_| false)
                .iter()
                .any(|finding| finding.contains("missing herdr-plugin/proqi.sh"))
        );
        assert!(
            manifest_findings("id = ", &version(), |_| true)
                .iter()
                .any(|finding| finding.contains("not valid TOML"))
        );
    }
}
