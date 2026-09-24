//! Claude Code plugin marketplace version and source agreement gate.
//!
//! `Cargo.toml` owns the version. Every plugin entry in
//! `.claude-plugin/marketplace.json` must carry that exact version and a
//! relative repository source, so a marketplace install follows the default
//! branch while updates are announced only when release preparation bumps the
//! Cargo version. A tag-pinned remote source is rejected because the tag does
//! not exist between release preparation and tag creation.

use std::{fs, path::Path};

use semver::Version;
use serde_json::Value;

pub(super) const MANIFEST: &str = ".claude-plugin/marketplace.json";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

pub(super) fn validate(root: &Path) -> Result<(), String> {
    let path = root.join(MANIFEST);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("Claude marketplace {MANIFEST} is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "Claude marketplace {MANIFEST} must be a regular file of at most {MAX_MANIFEST_BYTES} bytes"
        ));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("read Claude marketplace {MANIFEST}: {error}"))?;
    let manifest: Value = serde_json::from_str(&contents)
        .map_err(|error| format!("parse Claude marketplace {MANIFEST}: {error}"))?;
    validate_agreement(&manifest, &super::release::workspace_version(root)?)
}

fn validate_agreement(manifest: &Value, version: &Version) -> Result<(), String> {
    for location in ["version", "metadata/version"] {
        if manifest.pointer(&format!("/{location}")).is_some() {
            return Err(format!(
                "{MANIFEST} must not declare a marketplace-level `{location}`; the plugin entry owns the version"
            ));
        }
    }
    let plugins = manifest
        .get("plugins")
        .and_then(Value::as_array)
        .filter(|plugins| !plugins.is_empty())
        .ok_or_else(|| format!("{MANIFEST} must list at least one plugin"))?;
    for plugin in plugins {
        validate_plugin(plugin, version)?;
    }
    Ok(())
}

fn validate_plugin(plugin: &Value, version: &Version) -> Result<(), String> {
    let name = plugin
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{MANIFEST} has a plugin entry without a name"))?;
    let declared = plugin.get("version").and_then(Value::as_str);
    if declared != Some(version.to_string().as_str()) {
        return Err(format!(
            "{MANIFEST} plugin `{name}` version {declared:?} must exactly match Cargo version {version}"
        ));
    }
    let source = plugin.get("source").and_then(Value::as_str);
    let relative = source.is_some_and(|source| {
        source.starts_with("./")
            && !Path::new(source)
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
    });
    if !relative {
        return Err(format!(
            "{MANIFEST} plugin `{name}` must use a relative `./` repository source, not {source:?} or a pinned remote"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn version() -> Version {
        Version::new(1, 2, 3)
    }

    fn manifest(plugin: Value) -> Value {
        let mut manifest = json!({"name": "proqi", "owner": {"name": "Owner"}});
        manifest["plugins"] = Value::Array(vec![plugin]);
        manifest
    }

    #[test]
    fn matching_relative_entry_is_accepted() {
        let accepted = manifest(json!({"name": "proqi", "source": "./skills", "version": "1.2.3"}));
        assert_eq!(validate_agreement(&accepted, &version()), Ok(()));
    }

    #[test]
    fn missing_or_mismatched_versions_are_rejected() {
        for plugin in [
            json!({"name": "proqi", "source": "./skills"}),
            json!({"name": "proqi", "source": "./skills", "version": "1.2.2"}),
            json!({"name": "proqi", "source": "./skills", "version": "v1.2.3"}),
        ] {
            let error = validate_agreement(&manifest(plugin), &version()).expect_err("rejected");
            assert!(
                error.contains("must exactly match Cargo version 1.2.3"),
                "{error}"
            );
        }
    }

    #[test]
    fn marketplace_level_versions_are_rejected() {
        let mut duplicate =
            manifest(json!({"name": "proqi", "source": "./skills", "version": "1.2.3"}));
        duplicate["metadata"] = json!({"version": "1.2.3"});
        let error = validate_agreement(&duplicate, &version()).expect_err("rejected");
        assert!(error.contains("metadata/version"), "{error}");
    }

    #[test]
    fn remote_escaping_and_absent_sources_are_rejected() {
        for source in [
            json!({"source": "git-subdir", "url": "https://github.com/oborchers/proqi.git", "path": "skills", "ref": "v1.2.3"}),
            json!("skills"),
            json!("./skills/../.."),
            json!("/skills"),
        ] {
            let plugin = json!({"name": "proqi", "source": source, "version": "1.2.3"});
            let error = validate_agreement(&manifest(plugin), &version()).expect_err("rejected");
            assert!(error.contains("relative `./` repository source"), "{error}");
        }
        let error = validate_agreement(&json!({"plugins": []}), &version()).expect_err("rejected");
        assert!(error.contains("at least one plugin"), "{error}");
    }

    #[test]
    fn checked_in_manifest_agrees_with_the_workspace() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask manifest has a workspace parent");
        assert_eq!(validate(root), Ok(()));
    }
}
