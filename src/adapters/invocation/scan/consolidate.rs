use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::ports::invocation::{
    InvocationEntry, InvocationForm, InvocationHarness, InvocationKind,
};

use super::{RootShape, ScanRoot};

pub(super) struct ObservedEntry {
    entry: InvocationEntry,
    shared_base: Option<PathBuf>,
    canonical_agent_root: Option<PathBuf>,
    claude_aliases_agent_skill: bool,
}

pub(super) fn observe(root: &ScanRoot, definition: &Path, entry: InvocationEntry) -> ObservedEntry {
    let shared_base = shared_skills_base(root);
    let canonical_agent_root = canonical_agent_root(root, shared_base.as_deref());
    let claude_aliases_agent_skill =
        claude_aliases_agent_skill(root, definition, shared_base.as_deref());
    ObservedEntry {
        entry,
        shared_base,
        canonical_agent_root,
        claude_aliases_agent_skill,
    }
}

pub(super) fn entries(observations: Vec<ObservedEntry>) -> Vec<InvocationEntry> {
    let mut groups = BTreeMap::<PathBuf, Vec<ObservedEntry>>::new();
    for observation in observations {
        groups
            .entry(observation.entry.canonical_path.clone())
            .or_default()
            .push(observation);
    }
    groups.into_values().map(consolidate_group).collect()
}

fn consolidate_group(mut group: Vec<ObservedEntry>) -> InvocationEntry {
    group.sort_by(|left, right| entry_order(&left.entry, &right.entry));
    if let Some(entry) = shared_agent_skill(&group) {
        return entry;
    }
    group.remove(0).entry
}

fn shared_agent_skill(group: &[ObservedEntry]) -> Option<InvocationEntry> {
    let owner = group.iter().find(|observation| {
        observation.entry.source == InvocationHarness::AgentSkills
            && observation.entry.kind == InvocationKind::Skill
            && observation.canonical_agent_root.is_some()
    })?;
    let base = owner.shared_base.as_ref()?;
    let scope = owner.entry.scope;
    if group
        .iter()
        .any(|observation| observation.entry.scope != scope)
    {
        return None;
    }
    let canonical_agent_root = owner.canonical_agent_root.as_ref()?;
    let physically_agent_owned = owner.entry.canonical_path.starts_with(canonical_agent_root);
    let aliases = group.iter().filter(|observation| {
        observation.shared_base.as_ref() == Some(base)
            && matches!(
                observation.entry.source,
                InvocationHarness::AgentSkills | InvocationHarness::ClaudeCode
            )
            && observation.entry.kind == InvocationKind::Skill
            && (observation.entry.source == InvocationHarness::AgentSkills
                || physically_agent_owned
                || observation.claude_aliases_agent_skill)
    });
    let mut forms = aliases
        .clone()
        .flat_map(|observation| observation.entry.forms.iter().cloned())
        .collect::<Vec<_>>();
    if !aliases
        .map(|observation| observation.entry.source)
        .any(|source| source == InvocationHarness::ClaudeCode)
    {
        return None;
    }
    sort_forms(&mut forms);
    let mut entry = owner.entry.clone();
    entry.forms = forms;
    entry.precedence = group
        .iter()
        .map(|observation| observation.entry.precedence)
        .min()
        .unwrap_or(entry.precedence);
    Some(entry)
}

fn shared_skills_base(root: &ScanRoot) -> Option<PathBuf> {
    if root.plugin.is_some() || root.shape != RootShape::Skills {
        return None;
    }
    let parent = root.path.parent()?;
    let expected = match root.harness {
        InvocationHarness::AgentSkills => ".agents",
        InvocationHarness::ClaudeCode => ".claude",
        _ => return None,
    };
    if root.path.file_name()?.to_str()? != "skills" || parent.file_name()?.to_str()? != expected {
        return None;
    }
    parent.parent().map(Path::to_path_buf)
}

fn canonical_agent_root(root: &ScanRoot, base: Option<&Path>) -> Option<PathBuf> {
    if root.harness != InvocationHarness::AgentSkills {
        return None;
    }
    canonical_agent_root_for_base(base?)
}

fn canonical_agent_root_for_base(base: &Path) -> Option<PathBuf> {
    let agent_dir = base.join(".agents");
    let skills = agent_dir.join("skills");
    if is_symlink(&agent_dir) || is_symlink(&skills) {
        return None;
    }
    fs::canonicalize(skills).ok()
}

fn claude_aliases_agent_skill(root: &ScanRoot, definition: &Path, base: Option<&Path>) -> bool {
    if root.harness != InvocationHarness::ClaudeCode
        || root.plugin.is_some()
        || root.shape != RootShape::Skills
    {
        return false;
    }
    let Some(canonical_agent_root) = base.and_then(canonical_agent_root_for_base) else {
        return false;
    };
    if fs::canonicalize(&root.path).ok().as_ref() == Some(&canonical_agent_root) {
        return true;
    }
    definition
        .parent()
        .is_some_and(|directory| symlink_parent_is_within(directory, &canonical_agent_root))
}

fn symlink_parent_is_within(path: &Path, expected_root: &Path) -> bool {
    let Ok(target) = fs::read_link(path) else {
        return false;
    };
    let resolved = if target.is_absolute() {
        target
    } else {
        let Some(parent) = path.parent() else {
            return false;
        };
        parent.join(target)
    };
    resolved
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .is_some_and(|parent| parent.starts_with(expected_root))
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn sort_forms(forms: &mut Vec<InvocationForm>) {
    forms.sort_by(|left, right| {
        left.precedence
            .cmp(&right.precedence)
            .then_with(|| left.token.cmp(&right.token))
            .then_with(|| left.harness.cmp(&right.harness))
    });
    forms.dedup_by(|left, right| left.harness == right.harness && left.token == right.token);
}

fn entry_order(left: &InvocationEntry, right: &InvocationEntry) -> std::cmp::Ordering {
    left.precedence
        .cmp(&right.precedence)
        .then_with(|| left.source.cmp(&right.source))
        .then_with(|| left.scope.cmp(&right.scope))
        .then_with(|| left.name.cmp(&right.name))
}
