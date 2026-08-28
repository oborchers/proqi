use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::ports::invocation::{InvocationHarness, InvocationKind, InvocationScope};

use super::{metadata, roots::RootShape, scan::ScanRoot};

const MAX_REGISTRY_BYTES: u64 = 512 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PLUGINS: usize = 64;

pub(super) fn roots(home: &Path, cwd: &Path, existing: usize) -> Vec<ScanRoot> {
    let registry = home.join(".claude/plugins/installed_plugins.json");
    let Some(value) = bounded_json(&registry, MAX_REGISTRY_BYTES) else {
        return Vec::new();
    };
    let Some(plugins) = value.get("plugins").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for (registry_name, installations) in plugins.iter().take(MAX_PLUGINS) {
        for installation in installations.as_array().into_iter().flatten().take(4) {
            if output.len().saturating_add(existing) >= super::scan::MAX_ROOTS {
                return output;
            }
            let Some(path) = installation
                .get("installPath")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if path.len() > 1_024 {
                continue;
            }
            if !visible_from(cwd, installation) {
                continue;
            }
            output.extend(component_roots(&PathBuf::from(path), registry_name));
        }
    }
    output
}

fn visible_from(cwd: &Path, installation: &serde_json::Value) -> bool {
    let project_scoped = matches!(
        installation
            .get("scope")
            .and_then(serde_json::Value::as_str),
        Some("project" | "local")
    );
    if !project_scoped {
        return true;
    }
    installation
        .get("projectPath")
        .and_then(serde_json::Value::as_str)
        .map(Path::new)
        .is_some_and(|project| cwd.starts_with(project))
}

fn component_roots(base: &Path, registry_name: &str) -> Vec<ScanRoot> {
    let manifest = bounded_json(&base.join(".claude-plugin/plugin.json"), MAX_MANIFEST_BYTES);
    let plugin = manifest
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .and_then(metadata::clean_name)
        .or_else(|| {
            registry_name
                .split('@')
                .next()
                .and_then(metadata::clean_name)
        });
    let Some(plugin) = plugin else {
        return Vec::new();
    };
    let skills = manifest
        .as_ref()
        .and_then(|value| value.get("skills"))
        .map_or_else(|| vec![PathBuf::from("skills")], component_paths);
    let commands = manifest
        .as_ref()
        .and_then(|value| value.get("commands"))
        .map_or_else(|| vec![PathBuf::from("commands")], component_paths);
    let agents = manifest
        .as_ref()
        .and_then(|value| value.get("agents"))
        .map_or_else(|| vec![PathBuf::from("agents")], component_paths);
    let mut output = skills
        .into_iter()
        .map(|path| {
            component(
                base.join(path),
                &plugin,
                InvocationKind::Skill,
                RootShape::Skills,
            )
        })
        .collect::<Vec<_>>();
    output.extend(commands.into_iter().map(|path| {
        component(
            base.join(path),
            &plugin,
            InvocationKind::Command,
            RootShape::MarkdownCommands,
        )
    }));
    output.extend(agents.into_iter().map(|path| {
        component(
            base.join(path),
            &plugin,
            InvocationKind::Agent,
            RootShape::MarkdownAgents,
        )
    }));
    output
}

fn component_paths(value: &serde_json::Value) -> Vec<PathBuf> {
    let values = value
        .as_array()
        .map_or_else(|| vec![value], |items| items.iter().take(16).collect());
    values
        .into_iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| value.len() <= 1_024)
        .map(PathBuf::from)
        .filter(|path| {
            !path.is_absolute()
                && path
                    .components()
                    .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        })
        .collect()
}

fn component(path: PathBuf, plugin: &str, kind: InvocationKind, shape: RootShape) -> ScanRoot {
    ScanRoot {
        path,
        scope: InvocationScope::Plugin,
        harness: InvocationHarness::ClaudeCode,
        kind,
        shape,
        precedence: 70,
        plugin: Some(plugin.to_owned()),
        insertable: true,
    }
}

fn bounded_json(path: &Path, maximum: u64) -> Option<serde_json::Value> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > maximum {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}
