use std::{
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
};

use crate::ports::invocation::{
    InvocationCompleteness, InvocationHarness, InvocationIncompleteReason, InvocationKind,
    InvocationScope,
};

use super::scan::budget::WorkBudget;
use super::{metadata, roots::RootShape, scan::ScanRoot};

const MAX_REGISTRY_BYTES: u64 = 512 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub(super) const PRECEDENCE: u16 = 70;

pub(super) struct PluginRoots {
    pub(super) roots: Vec<ScanRoot>,
    pub(super) completeness: InvocationCompleteness,
}

pub(super) fn roots(home: &Path, cwd: &Path, budget: &mut WorkBudget) -> PluginRoots {
    let value = match registry(home, budget) {
        BoundedJson::Value(value) => value,
        BoundedJson::Oversized(observed) => {
            let mut completeness = InvocationCompleteness::Complete;
            completeness.add(InvocationIncompleteReason::RegistrySize {
                observed,
                limit: MAX_REGISTRY_BYTES,
            });
            return PluginRoots {
                roots: Vec::new(),
                completeness,
            };
        }
        BoundedJson::Unavailable => {
            return PluginRoots {
                roots: Vec::new(),
                completeness: InvocationCompleteness::Complete,
            };
        }
    };
    let Some(plugins) = value.get("plugins").and_then(serde_json::Value::as_object) else {
        return PluginRoots {
            roots: Vec::new(),
            completeness: InvocationCompleteness::Complete,
        };
    };
    let mut output = Vec::new();
    let mut oversized_manifests = 0usize;
    let mut largest_manifest = 0u64;
    for (registry_name, installations) in plugins {
        for installation in installations.as_array().into_iter().flatten() {
            if budget.should_stop() || budget.root_exhausted() {
                break;
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
            if !budget.visit_path() || budget.observe_cancellation() {
                break;
            }
            output.extend(component_roots(
                &PathBuf::from(path),
                registry_name,
                &mut oversized_manifests,
                &mut largest_manifest,
                budget,
            ));
        }
        if budget.should_stop() || budget.root_exhausted() {
            break;
        }
    }
    let mut completeness = InvocationCompleteness::Complete;
    if oversized_manifests > 0 {
        completeness.add(InvocationIncompleteReason::ManifestSize {
            observed: largest_manifest,
            limit: MAX_MANIFEST_BYTES,
            affected: oversized_manifests,
        });
    }
    PluginRoots {
        roots: output,
        completeness,
    }
}

fn registry(home: &Path, budget: &mut WorkBudget) -> BoundedJson {
    let path = home.join(".claude/plugins/installed_plugins.json");
    if !path.is_file() || !budget.visit_path() || budget.observe_cancellation() {
        return BoundedJson::Unavailable;
    }
    bounded_json(&path, MAX_REGISTRY_BYTES)
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

fn component_roots(
    base: &Path,
    registry_name: &str,
    oversized_manifests: &mut usize,
    largest_manifest: &mut u64,
    budget: &mut WorkBudget,
) -> Vec<ScanRoot> {
    let manifest = match bounded_json(&base.join(".claude-plugin/plugin.json"), MAX_MANIFEST_BYTES)
    {
        BoundedJson::Value(value) => Some(value),
        BoundedJson::Oversized(observed) => {
            *oversized_manifests = oversized_manifests.saturating_add(1);
            *largest_manifest = (*largest_manifest).max(observed);
            None
        }
        BoundedJson::Unavailable => None,
    };
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
    let mut output = Vec::new();
    append_components(
        &mut output,
        base,
        &plugin,
        skills,
        InvocationKind::Skill,
        RootShape::Skills,
        budget,
    );
    append_components(
        &mut output,
        base,
        &plugin,
        commands,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        budget,
    );
    append_components(
        &mut output,
        base,
        &plugin,
        agents,
        InvocationKind::Agent,
        RootShape::MarkdownAgents,
        budget,
    );
    output
}

fn append_components(
    output: &mut Vec<ScanRoot>,
    base: &Path,
    plugin: &str,
    paths: Vec<PathBuf>,
    kind: InvocationKind,
    shape: RootShape,
    budget: &mut WorkBudget,
) {
    for relative in paths {
        if budget.should_stop() || budget.root_exhausted() {
            return;
        }
        if !budget.visit_path() || budget.observe_cancellation() {
            return;
        }
        let path = base.join(relative);
        if !path.exists() {
            continue;
        }
        if !budget.admit_root() {
            return;
        }
        output.push(component(path, plugin, kind, shape));
    }
}

fn component_paths(value: &serde_json::Value) -> Vec<PathBuf> {
    let values = value
        .as_array()
        .map_or_else(|| vec![value], |items| items.iter().collect());
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
        precedence: PRECEDENCE,
        plugin: Some(plugin.to_owned()),
        insertable: true,
    }
}

enum BoundedJson {
    Value(serde_json::Value),
    Oversized(u64),
    Unavailable,
}

fn bounded_json(path: &Path, maximum: u64) -> BoundedJson {
    let Ok(file) = fs::File::open(path) else {
        return BoundedJson::Unavailable;
    };
    let Ok(metadata) = file.metadata() else {
        return BoundedJson::Unavailable;
    };
    if !metadata.is_file() {
        return BoundedJson::Unavailable;
    }
    if metadata.len() > maximum {
        return BoundedJson::Oversized(metadata.len());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    if file
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return BoundedJson::Unavailable;
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return BoundedJson::Oversized(maximum.saturating_add(1));
    }
    String::from_utf8(bytes)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .map_or(BoundedJson::Unavailable, BoundedJson::Value)
}
