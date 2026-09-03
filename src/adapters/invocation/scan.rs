use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::ports::invocation::{
    AdditionalInvocationRoot, InvocationCatalog, InvocationCatalogError, InvocationDiscovery,
    InvocationDiscoveryRequest, InvocationEntry, InvocationForm, InvocationHarness, InvocationKind,
    InvocationScope,
};

use super::{
    metadata,
    roots::{self, CompatibilityRoot, RootShape},
};

mod consolidate;

use consolidate::ObservedEntry;

pub(super) const MAX_ROOTS: usize = 128;
const MAX_DEPTH: usize = 6;
const MAX_ENTRIES: usize = 2_048;
const MAX_VISITED_PATHS: usize = 8_192;

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
}

impl FilesystemInvocationCatalog {
    /// Construct the production catalog from platform directories.
    #[must_use]
    pub fn system(additional: Vec<AdditionalInvocationRoot>) -> Self {
        Self {
            home: directories::BaseDirs::new().map(|directories| directories.home_dir().to_owned()),
            additional,
        }
    }

    /// Construct an isolated adapter for deterministic tests.
    #[must_use]
    pub const fn with_home(
        home: Option<PathBuf>,
        additional: Vec<AdditionalInvocationRoot>,
    ) -> Self {
        Self { home, additional }
    }
}

impl InvocationCatalog for FilesystemInvocationCatalog {
    fn discover(
        &mut self,
        request: InvocationDiscoveryRequest,
    ) -> Result<InvocationDiscovery, InvocationCatalogError> {
        let request_cwd = request.cwd;
        let cwd = fs::canonicalize(&request_cwd).unwrap_or_else(|_| request_cwd.clone());
        let mut roots = compatible_roots(&cwd, self.home.as_deref());
        roots.extend(additional_roots(&cwd, &self.additional));
        roots.retain(|root| root.path.exists());
        if roots.len() > MAX_ROOTS {
            return Err(InvocationCatalogError::RootBudget);
        }
        if let Some(home) = self.home.as_deref() {
            roots.extend(super::plugins::roots(home, &cwd, roots.len()));
        }
        roots.truncate(MAX_ROOTS);
        roots.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.path.cmp(&right.path))
        });
        let mut visited = 0;
        let mut observations = Vec::new();
        for root in roots {
            scan_root(&root, &mut observations, &mut visited);
            if observations.len() >= MAX_ENTRIES {
                break;
            }
        }
        let entries = consolidate::entries(observations);
        let (mut project, mut global): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| entry.scope == InvocationScope::Project);
        sort_entries(&mut project);
        sort_entries(&mut global);
        Ok(InvocationDiscovery {
            generation: request.generation,
            cwd: request_cwd,
            global,
            project,
        })
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

fn scan_root(root: &ScanRoot, entries: &mut Vec<ObservedEntry>, visited: &mut usize) {
    if entries.len() >= MAX_ENTRIES || *visited >= MAX_VISITED_PATHS {
        return;
    }
    if root.path.is_file() {
        match root.shape {
            RootShape::MarkdownCommands | RootShape::MarkdownAgents => {
                push_markdown(root, &root.path, None, entries);
            }
            RootShape::TomlAgents => push_toml(root, &root.path, entries),
            RootShape::Skills => {}
        }
        return;
    }
    if !root.path.is_dir() {
        return;
    }
    walk(root, &root.path, 0, entries, visited);
}

fn walk(
    root: &ScanRoot,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<ObservedEntry>,
    visited: &mut usize,
) {
    if depth > MAX_DEPTH || entries.len() >= MAX_ENTRIES || *visited >= MAX_VISITED_PATHS {
        return;
    }
    if root.shape == RootShape::Skills {
        let definition = directory.join("SKILL.md");
        if definition.is_file() {
            push_markdown(root, &definition, Some(directory), entries);
            return;
        }
    }
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    let remaining = MAX_VISITED_PATHS.saturating_sub(*visited);
    let mut children = read
        .filter_map(Result::ok)
        .take(remaining)
        .collect::<Vec<_>>();
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        *visited = visited.saturating_add(1);
        if entries.len() >= MAX_ENTRIES || *visited > MAX_VISITED_PATHS {
            return;
        }
        let path = child.path();
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(root, &path, depth.saturating_add(1), entries, visited);
        } else if file_type.is_symlink() && path.is_dir() && root.shape == RootShape::Skills {
            let definition = path.join("SKILL.md");
            if definition.is_file() {
                push_markdown(root, &definition, Some(&path), entries);
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some(extension(root))
        {
            match root.shape {
                RootShape::Skills => {}
                RootShape::MarkdownCommands | RootShape::MarkdownAgents => {
                    push_markdown(root, &path, None, entries);
                }
                RootShape::TomlAgents => push_toml(root, &path, entries),
            }
        }
    }
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
        );
        return;
    }
    let forms = forms(root, &name);
    push(root, definition, name, metadata.description, forms, entries);
}

fn push_toml(root: &ScanRoot, definition: &Path, entries: &mut Vec<ObservedEntry>) {
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
    );
}

fn push(
    root: &ScanRoot,
    definition: &Path,
    name: String,
    description: Option<String>,
    forms: Vec<InvocationForm>,
    entries: &mut Vec<ObservedEntry>,
) {
    let Ok(canonical_path) = fs::canonicalize(definition) else {
        return;
    };
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

fn forms(root: &ScanRoot, name: &str) -> Vec<InvocationForm> {
    if !root.insertable {
        return Vec::new();
    }
    let token = match (root.harness, root.kind) {
        (InvocationHarness::Codex | InvocationHarness::AgentSkills, InvocationKind::Skill) => {
            Some(format!("${name}"))
        }
        (InvocationHarness::ClaudeCode, InvocationKind::Skill | InvocationKind::Command) => Some(
            root.plugin
                .as_ref()
                .map_or_else(|| format!("/{name}"), |plugin| format!("/{plugin}:{name}")),
        ),
        (InvocationHarness::ClaudeCode, InvocationKind::Agent) => {
            Some(root.plugin.as_ref().map_or_else(
                || format!("@agent-{name}"),
                |plugin| format!("@agent-{plugin}:{name}"),
            ))
        }
        (
            InvocationHarness::OpenCode | InvocationHarness::Pi | InvocationHarness::Configured,
            InvocationKind::Command,
        ) => Some(format!("/{name}")),
        (InvocationHarness::OpenCode, InvocationKind::Agent) => Some(format!("@{name}")),
        (InvocationHarness::Pi, InvocationKind::Skill) => Some(format!("/skill:{name}")),
        (InvocationHarness::Configured, InvocationKind::Skill) => Some(format!("${name}")),
        _ => None,
    };
    token
        .map(|token| {
            vec![InvocationForm {
                harness: if root.harness == InvocationHarness::AgentSkills {
                    InvocationHarness::Codex
                } else {
                    root.harness
                },
                token,
                precedence: root.precedence,
            }]
        })
        .unwrap_or_default()
}

fn sort_entries(entries: &mut [InvocationEntry]) {
    entries.sort_by(|left, right| {
        left.precedence
            .cmp(&right.precedence)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.canonical_path.cmp(&right.canonical_path))
    });
}
