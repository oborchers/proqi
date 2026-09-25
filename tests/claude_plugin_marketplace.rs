//! Claude Code plugin marketplace distribution contract.
//!
//! This suite owns one core contract: `.claude-plugin/marketplace.json`
//! publishes the shipped `skills/` tree as the `proqi@proqi` plugin without a
//! second copy of any skill, so a marketplace install and `npx skills add`
//! deliver byte-identical skill files. Field requirements follow
//! <https://code.claude.com/docs/en/plugin-marketplaces#marketplace-schema>.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

const MANIFEST: &str = include_str!("../.claude-plugin/marketplace.json");
const README: &str = include_str!("../README.md");
const CLI_REFERENCE: &str = include_str!("../docs/reference/cli.md");
const GETTING_STARTED: &str = include_str!("../docs/getting-started.md");
const FEATURES: &str = include_str!("../docs/reference/features.md");
const RELEASING: &str = include_str!("../docs/RELEASING.md");

/// Names Claude Code reserves for official marketplaces.
const RESERVED: &[&str] = &[
    "claude-code-marketplace",
    "claude-code-plugins",
    "claude-plugins-official",
    "claude-plugins-community",
    "claude-community",
    "anthropic-marketplace",
    "anthropic-plugins",
    "agent-skills",
    "anthropic-agent-skills",
    "npm",
    "pip",
    "uv",
    "cargo",
    "github",
    "gh",
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("marketplace JSON")
}

fn plugin() -> Value {
    let manifest = manifest();
    let plugins = manifest["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1, "exactly one published plugin");
    plugins[0].clone()
}

fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Resolves a documented `./` relative path without allowing an escape.
fn relative(base: &Path, path: &str) -> PathBuf {
    let stripped = path.strip_prefix("./").expect("path starts with ./");
    let relative = Path::new(stripped);
    assert!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "path {path} must stay inside its base"
    );
    base.join(relative)
}

fn plugin_root() -> PathBuf {
    relative(
        root(),
        plugin()["source"].as_str().expect("relative source"),
    )
}

fn shipped_skill_directories() -> BTreeSet<PathBuf> {
    let plugin_root = plugin_root();
    plugin()["skills"]
        .as_array()
        .expect("explicit skills list")
        .iter()
        .map(|path| relative(&plugin_root, path.as_str().expect("skill path")))
        .collect()
}

#[test]
fn marketplace_has_the_documented_required_fields() {
    let manifest = manifest();
    let name = manifest["name"].as_str().expect("marketplace name");
    assert_eq!(name, "proqi");
    assert!(is_kebab(name));
    assert!(!RESERVED.contains(&name));
    assert!(
        manifest["owner"]["name"]
            .as_str()
            .is_some_and(|owner| !owner.is_empty())
    );
    assert!(
        manifest["description"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );

    let plugin = plugin();
    assert_eq!(plugin["name"].as_str(), Some("proqi"));
    assert!(is_kebab(plugin["name"].as_str().expect("plugin name")));
    assert_eq!(plugin["strict"], Value::Bool(false));
    assert_eq!(plugin["version"].as_str(), Some(env!("CARGO_PKG_VERSION")));
    assert!(manifest.get("version").is_none());
    assert!(manifest.pointer("/metadata/version").is_none());
}

#[test]
fn plugin_source_is_the_shipped_skills_tree_and_excludes_internal_skills() {
    let plugin_root = plugin_root();
    assert_eq!(plugin_root, root().join("skills"));
    for internal in [".claude", ".agents", "src", "tests", "xtask"] {
        assert!(!plugin_root.starts_with(root().join(internal)));
        assert!(!root().join(internal).starts_with(&plugin_root));
    }
}

#[test]
fn every_public_skill_is_shipped_exactly_once() {
    let public = fs::read_dir(root().join("skills"))
        .expect("skills directory")
        .map(|entry| entry.expect("skill entry").path())
        .filter(|path| path.join("SKILL.md").is_file())
        .collect::<BTreeSet<_>>();
    let shipped = shipped_skill_directories();
    assert_eq!(shipped, public);
    assert_eq!(
        plugin()["skills"].as_array().expect("skills").len(),
        shipped.len()
    );
    for directory in shipped {
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .expect("name");
        let skill = fs::read_to_string(directory.join("SKILL.md")).expect("SKILL.md");
        assert!(
            skill.starts_with(&format!("---\nname: {name}\ndescription: ")),
            "{name} frontmatter name must match its directory"
        );
    }
}

#[test]
fn plugin_tree_has_no_manifest_conflict_or_link_to_dereference() {
    fn walk(directory: &Path) {
        for entry in fs::read_dir(directory).expect("plugin directory") {
            let path = entry.expect("plugin entry").path();
            let metadata = fs::symlink_metadata(&path).expect("metadata");
            assert!(!metadata.file_type().is_symlink(), "{}", path.display());
            assert_ne!(
                path.file_name().and_then(|name| name.to_str()),
                Some("plugin.json")
            );
            assert_ne!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".claude-plugin")
            );
            if metadata.is_dir() {
                walk(&path);
            }
        }
    }
    walk(&plugin_root());
}

#[test]
fn both_install_paths_read_the_same_physical_skill_files() {
    for (directory, expected) in [
        ("proqi", include_str!("../skills/proqi/SKILL.md")),
        (
            "proqi-debug",
            include_str!("../skills/proqi-debug/SKILL.md"),
        ),
    ] {
        let npx = root().join("skills").join(directory).join("SKILL.md");
        let plugin = shipped_skill_directories()
            .into_iter()
            .find(|path| path.ends_with(directory))
            .expect("shipped skill")
            .join("SKILL.md");
        assert_eq!(
            fs::canonicalize(&plugin).expect("plugin path"),
            fs::canonicalize(&npx).expect("npx path")
        );
        assert_eq!(fs::read_to_string(plugin).expect("plugin skill"), expected);
    }
}

#[test]
fn every_repository_skill_name_has_one_physical_definition() {
    let mut definitions = Vec::new();
    for skill_root in ["skills", ".agents", ".claude", ".claude-plugin"] {
        collect_skill_definitions(&root().join(skill_root), &mut definitions);
    }
    let mut names = BTreeSet::new();
    for path in definitions {
        let contents = fs::read_to_string(&path).expect("skill definition");
        let name = frontmatter_name(&contents)
            .unwrap_or_else(|| panic!("{} has no frontmatter name", path.display()));
        assert!(
            names.insert(name.to_owned()),
            "duplicate skill {name} at {}",
            path.display()
        );
    }
    assert!(names.contains("proqi") && names.contains("proqi-debug"));
}

/// Reads the `name` field from a skill's leading frontmatter block.
fn frontmatter_name(contents: &str) -> Option<&str> {
    let block = contents.strip_prefix("---\n")?.split_once("\n---")?.0;
    block
        .lines()
        .find_map(|line| line.strip_prefix("name:"))
        .map(str::trim)
}

/// Collects physical `SKILL.md` files without following symlinked aliases.
fn collect_skill_definitions(directory: &Path, definitions: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("skill root entry").path();
        let metadata = fs::symlink_metadata(&path).expect("skill root metadata");
        if metadata.is_dir() {
            collect_skill_definitions(&path, definitions);
        } else if metadata.is_file() && path.file_name().is_some_and(|name| name == "SKILL.md") {
            definitions.push(path);
        }
    }
}

#[test]
fn both_install_paths_are_documented() {
    for document in [README, CLI_REFERENCE] {
        assert!(document.contains("/plugin update proqi@proqi"));
    }
    for document in [README, CLI_REFERENCE, GETTING_STARTED] {
        assert!(document.contains("/plugin marketplace add oborchers/proqi"));
        assert!(document.contains("/plugin install proqi@proqi"));
        assert!(document.contains("npx skills add oborchers/proqi --skill proqi"));
    }
    assert!(FEATURES.contains("proqi@proqi"));
    assert!(RELEASING.contains(".claude-plugin/marketplace.json"));
}
