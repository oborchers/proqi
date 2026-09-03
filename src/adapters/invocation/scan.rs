use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::ports::invocation::{
    AdditionalInvocationRoot, InvocationCancellation, InvocationCatalog, InvocationCompleteness,
    InvocationDiscovery, InvocationDiscoveryRequest, InvocationEntry, InvocationForm,
    InvocationHarness, InvocationKind, InvocationScope,
};

use super::{
    metadata,
    roots::{self, CompatibilityRoot, RootShape},
};

pub(super) mod budget;
mod consolidate;
mod forms;

use budget::{WORK_BUDGET, WorkBudget};
use consolidate::ObservedEntry;

#[derive(Clone)]
pub(super) struct ScanRoot {
    pub(super) path: PathBuf,
    pub(super) scope: InvocationScope,
    pub(super) harness: InvocationHarness,
    pub(super) kind: InvocationKind,
    pub(super) shape: RootShape,
    pub(super) precedence: u16,
    pub(super) plugin: Option<String>,
    pub(super) insertable: bool,
}

/// Filesystem adapter with injected global and configured roots.
pub struct FilesystemInvocationCatalog {
    home: Option<PathBuf>,
    additional: Vec<AdditionalInvocationRoot>,
    cancellation: Arc<dyn InvocationCancellation>,
}

impl FilesystemInvocationCatalog {
    /// Construct the production catalog from platform directories.
    #[must_use]
    pub fn system(additional: Vec<AdditionalInvocationRoot>) -> Self {
        Self {
            home: directories::BaseDirs::new().map(|directories| directories.home_dir().to_owned()),
            additional,
            cancellation: Arc::new(()),
        }
    }

    /// Construct the production catalog with shared runtime cancellation.
    #[must_use]
    pub fn cancellable(
        additional: Vec<AdditionalInvocationRoot>,
        cancellation: Arc<dyn InvocationCancellation>,
    ) -> Self {
        Self {
            home: directories::BaseDirs::new().map(|directories| directories.home_dir().to_owned()),
            additional,
            cancellation,
        }
    }

    /// Construct an isolated adapter for deterministic tests.
    #[must_use]
    pub fn with_home(home: Option<PathBuf>, additional: Vec<AdditionalInvocationRoot>) -> Self {
        Self {
            home,
            additional,
            cancellation: Arc::new(()),
        }
    }

    #[cfg(test)]
    pub(super) fn with_home_and_cancellation(
        home: Option<PathBuf>,
        cancellation: Arc<dyn InvocationCancellation>,
    ) -> Self {
        Self {
            home,
            additional: Vec::new(),
            cancellation,
        }
    }
}

impl InvocationCatalog for FilesystemInvocationCatalog {
    fn discover(&mut self, request: InvocationDiscoveryRequest) -> InvocationDiscovery {
        let request_cwd = request.cwd;
        let cwd = fs::canonicalize(&request_cwd).unwrap_or_else(|_| request_cwd.clone());
        let mut budget = WorkBudget::new(Arc::clone(&self.cancellation));
        if budget.observe_cancellation() {
            return InvocationDiscovery {
                generation: request.generation,
                cwd: request_cwd,
                global: Vec::new(),
                project: Vec::new(),
                completeness: budget.finish(&InvocationCompleteness::default()),
            };
        }
        let mut roots = compatible_roots(&cwd, self.home.as_deref());
        roots.extend(additional_roots(&cwd, &self.additional));
        roots.retain(|root| root.path.exists());
        roots.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.path.cmp(&right.path))
        });
        budget.admit_initial_roots(roots.len());
        roots.truncate(WORK_BUDGET.roots);
        let late_roots = roots
            .split_off(roots.partition_point(|root| root.precedence < super::plugins::PRECEDENCE));
        let mut observations = Vec::new();
        for root in roots {
            scan_root(&root, &mut observations, &mut budget);
            if budget.should_stop() {
                break;
            }
        }
        let plugins = if budget.should_stop() || budget.root_exhausted() {
            super::plugins::PluginRoots {
                roots: Vec::new(),
                completeness: InvocationCompleteness::default(),
            }
        } else {
            self.home.as_deref().map_or_else(
                || super::plugins::PluginRoots {
                    roots: Vec::new(),
                    completeness: InvocationCompleteness::default(),
                },
                |home| super::plugins::roots(home, &cwd, &mut budget),
            )
        };
        let mut remaining_roots = late_roots;
        remaining_roots.extend(plugins.roots);
        remaining_roots.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.path.cmp(&right.path))
        });
        for root in remaining_roots {
            scan_root(&root, &mut observations, &mut budget);
            if budget.should_stop() {
                break;
            }
        }
        let entries = consolidate::entries(observations);
        let (mut project, mut global): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| entry.scope == InvocationScope::Project);
        consolidate::sort_entries(&mut project);
        consolidate::sort_entries(&mut global);
        InvocationDiscovery {
            generation: request.generation,
            cwd: request_cwd,
            global,
            project,
            completeness: budget.finish(&plugins.completeness),
        }
    }
}

fn compatible_roots(cwd: &Path, home: Option<&Path>) -> Vec<ScanRoot> {
    let project_bases = roots::project_bases(cwd);
    roots::COMPATIBILITY_ROOTS
        .iter()
        .flat_map(|spec| match spec.scope {
            InvocationScope::Project => project_bases
                .iter()
                .enumerate()
                .map(|(distance, base)| from_spec(base, spec, distance))
                .collect::<Vec<_>>(),
            InvocationScope::Global => home
                .map(|base| vec![from_spec(base, spec, 0)])
                .unwrap_or_default(),
            InvocationScope::Plugin => Vec::new(),
        })
        .collect()
}

fn from_spec(base: &Path, spec: &CompatibilityRoot, distance: usize) -> ScanRoot {
    ScanRoot {
        path: base.join(spec.relative),
        scope: spec.scope,
        harness: spec.harness,
        kind: spec.kind,
        shape: spec.shape,
        precedence: spec
            .precedence
            .saturating_add(u16::try_from(distance).unwrap_or(u16::MAX)),
        plugin: None,
        insertable: spec.insertable,
    }
}

fn additional_roots(cwd: &Path, additional: &[AdditionalInvocationRoot]) -> Vec<ScanRoot> {
    additional
        .iter()
        .filter(|root| root.scope != InvocationScope::Global || root.path.is_absolute())
        .map(|root| ScanRoot {
            path: if root.path.is_absolute() {
                root.path.clone()
            } else {
                cwd.join(&root.path)
            },
            scope: root.scope,
            harness: root.harness,
            kind: root.kind,
            shape: match root.kind {
                InvocationKind::Skill => RootShape::Skills,
                InvocationKind::Command => RootShape::MarkdownCommands,
                InvocationKind::Agent if root.harness == InvocationHarness::Codex => {
                    RootShape::TomlAgents
                }
                InvocationKind::Agent => RootShape::MarkdownAgents,
            },
            precedence: 60,
            plugin: None,
            insertable: true,
        })
        .collect()
}

fn scan_root(root: &ScanRoot, entries: &mut Vec<ObservedEntry>, budget: &mut WorkBudget) {
    if budget.observe_cancellation() || budget.should_stop() {
        return;
    }
    if root.path.is_file() {
        match root.shape {
            RootShape::MarkdownCommands | RootShape::MarkdownAgents => {
                push_markdown(root, &root.path, None, entries, budget);
            }
            RootShape::TomlAgents => push_toml(root, &root.path, entries, budget),
            RootShape::Skills => {}
        }
        return;
    }
    if !root.path.is_dir() {
        return;
    }
    walk(root, &root.path, 0, entries, budget);
}

fn walk(
    root: &ScanRoot,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<ObservedEntry>,
    budget: &mut WorkBudget,
) {
    if budget.observe_cancellation() || budget.should_stop() {
        return;
    }
    if depth > WORK_BUDGET.recursive_depth {
        budget.note_depth(depth);
        return;
    }
    if root.shape == RootShape::Skills {
        let definition = directory.join("SKILL.md");
        if definition.is_file() {
            push_markdown(root, &definition, Some(directory), entries, budget);
            return;
        }
    }
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    for child in bounded_children(read, budget) {
        if !budget.visit_path() || budget.observe_cancellation() {
            return;
        }
        let path = child.path();
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(root, &path, depth.saturating_add(1), entries, budget);
        } else if file_type.is_symlink() && path.is_dir() && root.shape == RootShape::Skills {
            let definition = path.join("SKILL.md");
            if definition.is_file() {
                push_markdown(root, &definition, Some(&path), entries, budget);
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some(extension(root))
        {
            match root.shape {
                RootShape::Skills => {}
                RootShape::MarkdownCommands | RootShape::MarkdownAgents => {
                    push_markdown(root, &path, None, entries, budget);
                }
                RootShape::TomlAgents => push_toml(root, &path, entries, budget),
            }
        }
        if budget.should_stop() {
            return;
        }
    }
}

fn bounded_children(read: fs::ReadDir, budget: &mut WorkBudget) -> Vec<fs::DirEntry> {
    let remaining = budget.remaining_paths();
    let mut children = BTreeMap::new();
    let mut overflow = false;
    for child in read.filter_map(Result::ok) {
        if budget.observe_cancellation() {
            break;
        }
        if children.len() < remaining {
            children.insert(child.file_name(), child);
            continue;
        }
        overflow = true;
        let name = child.file_name();
        if children
            .last_key_value()
            .is_some_and(|(largest, _)| name < *largest)
        {
            children.pop_last();
            children.insert(name, child);
        }
    }
    if overflow {
        budget.note_path_overflow();
    }
    children.into_values().collect()
}

const fn extension(root: &ScanRoot) -> &'static str {
    match root.shape {
        RootShape::TomlAgents => "toml",
        RootShape::Skills | RootShape::MarkdownCommands | RootShape::MarkdownAgents => "md",
    }
}

fn push_markdown(
    root: &ScanRoot,
    definition: &Path,
    skill_directory: Option<&Path>,
    entries: &mut Vec<ObservedEntry>,
    budget: &mut WorkBudget,
) {
    let Ok(file_metadata) = fs::metadata(definition) else {
        return;
    };
    if !file_metadata.is_file() {
        return;
    }
    let metadata = match metadata::markdown(definition) {
        metadata::MarkdownMetadata::Parsed(metadata) => metadata,
        metadata::MarkdownMetadata::Absent if root.shape == RootShape::MarkdownCommands => {
            metadata::Metadata::default()
        }
        metadata::MarkdownMetadata::Absent | metadata::MarkdownMetadata::Invalid => return,
    };
    if metadata.hidden {
        return;
    }
    let has_description = metadata.description.is_some();
    let declared_name = metadata.name.clone();
    let name = match root.shape {
        RootShape::Skills => metadata.name,
        RootShape::MarkdownAgents => metadata.name.or_else(|| {
            skill_directory
                .and_then(|directory| directory.file_name())
                .and_then(|name| name.to_str())
                .and_then(metadata::clean_name)
                .or_else(|| metadata::filename_name(definition))
        }),
        RootShape::MarkdownCommands if root.path.is_file() => metadata::filename_name(definition),
        RootShape::MarkdownCommands => metadata::command_name(&root.path, definition, root.harness),
        RootShape::TomlAgents => None,
    };
    let Some(name) = name else {
        return;
    };
    if root.shape == RootShape::Skills && !has_description {
        return;
    }
    if root.shape == RootShape::MarkdownAgents
        && (!has_description
            || (root.harness == InvocationHarness::ClaudeCode && declared_name.is_none()))
    {
        return;
    }
    if root.shape == RootShape::MarkdownAgents
        && root.harness == InvocationHarness::OpenCode
        && metadata.mode.as_deref() == Some("primary")
    {
        push(
            root,
            definition,
            name,
            metadata.description,
            Vec::new(),
            entries,
            budget,
        );
        return;
    }
    let forms = forms::for_entry(root, &name);
    push(
        root,
        definition,
        name,
        metadata.description,
        forms,
        entries,
        budget,
    );
}

fn push_toml(
    root: &ScanRoot,
    definition: &Path,
    entries: &mut Vec<ObservedEntry>,
    budget: &mut WorkBudget,
) {
    let Some(metadata) = metadata::toml_agent(definition) else {
        return;
    };
    let Some(name) = metadata
        .name
        .or_else(|| metadata::filename_name(definition))
    else {
        return;
    };
    push(
        root,
        definition,
        name,
        metadata.description,
        Vec::new(),
        entries,
        budget,
    );
}

fn push(
    root: &ScanRoot,
    definition: &Path,
    name: String,
    description: Option<String>,
    forms: Vec<InvocationForm>,
    entries: &mut Vec<ObservedEntry>,
    budget: &mut WorkBudget,
) {
    let Ok(canonical_path) = fs::canonicalize(definition) else {
        return;
    };
    if !budget.admit_entry() {
        return;
    }
    entries.push(consolidate::observe(
        root,
        definition,
        InvocationEntry {
            name,
            description,
            kind: root.kind,
            scope: root.scope,
            source: root.harness,
            forms,
            canonical_path,
            precedence: root.precedence,
        },
    ));
}
