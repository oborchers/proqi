//! Truthful completeness at every filesystem and Claude plugin boundary.

use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::ports::invocation::{
    AdditionalInvocationRoot, InvocationCatalog as _, InvocationDiscoveryRequest,
    InvocationHarness, InvocationIncompleteReason, InvocationKind, InvocationScope,
};
use tempfile::TempDir;

use super::{discover, write};
use crate::adapters::invocation::FilesystemInvocationCatalog;

#[test]
fn more_than_sixty_four_plugins_and_every_installation_and_component_are_scanned() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let mut plugins = serde_json::Map::new();
    for index in 0..65 {
        let root = fixture.path().join(format!("plugins/p{index:03}"));
        write(
            &root.join(format!("commands/c{index:03}.md")),
            "---\ndescription: Plugin command\n---\nbody",
        );
        plugins.insert(
            format!("p{index:03}@catalog"),
            serde_json::json!([{"installPath": root}]),
        );
    }
    let multi = fixture.path().join("plugins/multi");
    for index in 0..5 {
        let install = multi.join(format!("install-{index}"));
        write(
            &install.join(format!("commands/install-{index}.md")),
            "---\ndescription: Installation command\n---\nbody",
        );
    }
    let component = fixture.path().join("plugins/components");
    let component_paths = (0..17)
        .map(|index| format!("commands-{index:02}"))
        .collect::<Vec<_>>();
    write(
        &component.join(".claude-plugin/plugin.json"),
        &serde_json::json!({"name":"components","commands":component_paths}).to_string(),
    );
    for index in 0..17 {
        write(
            &component.join(format!("commands-{index:02}/item-{index:02}.md")),
            "---\ndescription: Component command\n---\nbody",
        );
    }
    plugins.insert(
        "multi@catalog".to_owned(),
        serde_json::Value::Array(
            (0..5)
                .map(|index| {
                    serde_json::json!({"installPath": multi.join(format!("install-{index}"))})
                })
                .collect(),
        ),
    );
    plugins.insert(
        "components@catalog".to_owned(),
        serde_json::json!([{"installPath": component}]),
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":plugins}).to_string(),
    );

    let result = discover(&home, &cwd);
    let tokens = result
        .global
        .iter()
        .flat_map(|entry| entry.forms.iter().map(|form| form.token.as_str()))
        .collect::<Vec<_>>();
    assert!(result.completeness.is_complete());
    assert!(tokens.contains(&"/p064:c064"));
    assert!((0..5).all(|index| tokens.contains(&format!("/multi:install-{index}").as_str())));
    assert!((0..17).all(|index| tokens.contains(&format!("/components:item-{index:02}").as_str())));
}

#[test]
fn repository_root_is_found_more_than_sixteen_ancestors_above_the_cwd() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let repo = fixture.path().join("repo");
    write(&repo.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &repo.join(".claude/commands/root-command.md"),
        "---\ndescription: Root command\n---\nbody",
    );
    let cwd = (0..20).fold(repo.clone(), |path, index| path.join(format!("d{index}")));
    fs::create_dir_all(&cwd).expect("deep cwd");

    let result = discover(&home, &cwd);

    assert!(result.completeness.is_complete());
    assert!(
        result
            .project
            .iter()
            .any(|entry| entry.name == "root-command")
    );
}

#[test]
fn ancestor_discovery_without_a_repository_stops_only_at_the_filesystem_root() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let parent = fixture.path().join("ordinary");
    write(
        &parent.join(".claude/commands/ancestor.md"),
        "---\ndescription: Ancestor command\n---\nbody",
    );
    let cwd = (0..20).fold(parent, |path, index| path.join(format!("d{index}")));
    fs::create_dir_all(&cwd).expect("deep cwd");

    let result = discover(&home, &cwd);

    assert!(result.completeness.is_complete());
    assert!(result.project.iter().any(|entry| entry.name == "ancestor"));
}

#[test]
fn root_budget_boundary_and_next_root_are_truthful_and_deterministic() {
    let fixture = TempDir::new().expect("tempdir");
    let roots = (0..129)
        .map(|index| {
            let path = fixture.path().join(format!("root-{index:03}.md"));
            write(&path, "---\ndescription: Configured command\n---\nbody");
            AdditionalInvocationRoot {
                path,
                kind: InvocationKind::Command,
                harness: InvocationHarness::ClaudeCode,
                scope: InvocationScope::Global,
            }
        })
        .collect::<Vec<_>>();
    let request = InvocationDiscoveryRequest {
        generation: 1,
        cwd: fixture.path().to_owned(),
    };
    let boundary = FilesystemInvocationCatalog::with_home(None, roots[..128].to_vec())
        .discover(request.clone());
    let overflow = FilesystemInvocationCatalog::with_home(None, roots).discover(request);

    assert!(boundary.completeness.is_complete());
    assert_eq!(boundary.global.len(), 128);
    assert_eq!(overflow.global, boundary.global);
    assert!(matches!(
        overflow.completeness.reasons(),
        [InvocationIncompleteReason::RootBudget {
            observed: 129,
            limit: 128
        }]
    ));
}

#[test]
fn plugin_component_roots_share_the_root_budget_and_retain_its_prefix() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugin");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let component_paths = (0..129)
        .map(|index| format!("commands-{index:03}"))
        .collect::<Vec<_>>();
    for path in &component_paths {
        write(
            &plugin.join(path).join("item.md"),
            "---\ndescription: Plugin command\n---\nbody",
        );
    }
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"bounded@catalog":[{"installPath":plugin}]}}).to_string(),
    );
    let manifest = plugin.join(".claude-plugin/plugin.json");
    write(
        &manifest,
        &serde_json::json!({"name":"bounded","commands":&component_paths[..128]}).to_string(),
    );
    let boundary = discover(&home, &cwd);
    write(
        &manifest,
        &serde_json::json!({"name":"bounded","commands":component_paths}).to_string(),
    );
    let overflow = discover(&home, &cwd);

    assert!(boundary.completeness.is_complete());
    assert_eq!(boundary.global.len(), 128);
    assert_eq!(overflow.global, boundary.global);
    assert!(matches!(
        overflow.completeness.reasons(),
        [InvocationIncompleteReason::RootBudget {
            observed: 129,
            limit: 128
        }]
    ));
}

#[test]
fn plugin_installation_expansion_shares_the_path_budget_and_keeps_healthy_entries() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &home.join(".agents/skills/healthy/SKILL.md"),
        "---\nname: healthy\ndescription: Healthy source\n---\nbody",
    );
    let installations = (0..9_000)
        .map(|_| serde_json::json!({"installPath":"missing"}))
        .collect::<Vec<_>>();
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"@":installations}}).to_string(),
    );

    let result = discover(&home, &cwd);

    assert!(result.global.iter().any(|entry| entry.name == "healthy"));
    assert!(matches!(
        result.completeness.reasons(),
        [InvocationIncompleteReason::PathBudget {
            observed: 8_193,
            limit: 8_192
        }]
    ));
}

#[test]
fn cancellation_is_observed_during_plugin_expansion() {
    struct CancelAfter(AtomicUsize);

    impl crate::ports::invocation::InvocationCancellation for CancelAfter {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 10
        }
    }

    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let installations = (0..100)
        .map(|_| serde_json::json!({"installPath":"missing"}))
        .collect::<Vec<_>>();
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"cancelled@catalog":installations}}).to_string(),
    );
    let mut catalog = FilesystemInvocationCatalog::with_home_and_cancellation(
        Some(home),
        Arc::new(CancelAfter(AtomicUsize::new(0))),
    );

    let result = catalog.discover(InvocationDiscoveryRequest { generation: 1, cwd });

    assert!(matches!(
        result.completeness.reasons(),
        [InvocationIncompleteReason::Cancelled {
            stage: crate::ports::invocation::InvocationDiscoveryStage::Filesystem
        }]
    ));
}

#[test]
fn visited_path_boundary_and_next_path_are_truthful() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let root = cwd.join(".opencode/commands");
    write(
        &root.join("0000-boundary.md"),
        "---\ndescription: Retained command\n---\nbody",
    );
    for index in 0..8_191 {
        write(&root.join(format!("ignored-{index:04}.txt")), "ignored");
    }
    let boundary = discover(&home, &cwd);
    write(
        &root.join("zzzz-overflow.md"),
        "---\ndescription: Omitted command\n---\nbody",
    );
    let overflow = discover(&home, &cwd);

    assert!(boundary.completeness.is_complete());
    assert_eq!(overflow.project, boundary.project);
    assert_eq!(boundary.project[0].name, "0000-boundary");
    assert!(matches!(
        overflow.completeness.reasons(),
        [InvocationIncompleteReason::PathBudget {
            observed: 8_193,
            limit: 8_192
        }]
    ));
}

#[test]
fn recursive_depth_boundary_retains_entries_and_reports_the_next_level() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let root = cwd.join(".opencode/commands");
    let at_boundary = (1..=6).fold(root.clone(), |path, index| path.join(format!("d{index}")));
    write(
        &at_boundary.join("boundary.md"),
        "---\ndescription: Boundary\n---\nbody",
    );
    let boundary = discover(&home, &cwd);
    write(
        &at_boundary.join("d7/hidden.md"),
        "---\ndescription: Hidden\n---\nbody",
    );
    let overflow = discover(&home, &cwd);

    assert!(boundary.completeness.is_complete());
    assert!(
        overflow
            .project
            .iter()
            .any(|entry| entry.name.ends_with("boundary"))
    );
    assert!(matches!(
        overflow.completeness.reasons(),
        [InvocationIncompleteReason::RecursiveDepth {
            observed: 7,
            limit: 6
        }]
    ));
}

#[test]
fn oversized_registry_and_manifest_keep_healthy_sources_and_exact_reasons() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &home.join(".agents/skills/healthy/SKILL.md"),
        "---\nname: healthy\ndescription: Healthy source\n---\nbody",
    );
    let registry = format!(
        "{{\"plugins\":{{}},\"padding\":\"{}\"}}",
        "x".repeat(512 * 1024)
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &registry,
    );
    let oversized_registry = discover(&home, &cwd);
    assert!(
        oversized_registry
            .global
            .iter()
            .any(|entry| entry.name == "healthy")
    );
    assert!(matches!(
        oversized_registry.completeness.reasons(),
        [InvocationIncompleteReason::RegistrySize { limit, .. }] if *limit == 512 * 1024
    ));

    let plugin = fixture.path().join("plugin");
    write(
        &plugin.join("commands/default.md"),
        "---\ndescription: Default command\n---\nbody",
    );
    write(
        &plugin.join(".claude-plugin/plugin.json"),
        &format!(
            "{{\"commands\":\"custom\",\"padding\":\"{}\"}}",
            "x".repeat(64 * 1024)
        ),
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"oversized@catalog":[{"installPath":plugin}]}}).to_string(),
    );
    let oversized_manifest = discover(&home, &cwd);
    assert!(
        oversized_manifest
            .global
            .iter()
            .any(|entry| entry.name == "default")
    );
    assert!(
        oversized_manifest
            .global
            .iter()
            .any(|entry| entry.name == "healthy")
    );
    assert!(matches!(
        oversized_manifest.completeness.reasons(),
        [InvocationIncompleteReason::ManifestSize {
            limit,
            affected: 1,
            ..
        }] if *limit == 64 * 1024
    ));
}

#[test]
fn unlimited_manifest_components_still_cannot_escape_the_installation() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugins/safe");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &plugin.join(".claude-plugin/plugin.json"),
        &serde_json::json!({
            "name":"safe",
            "commands":["commands", "../escaped", "/absolute"]
        })
        .to_string(),
    );
    write(
        &plugin.join("commands/inside.md"),
        "---\ndescription: Safe command\n---\nbody",
    );
    write(
        &plugin.join("../escaped/outside.md"),
        "---\ndescription: Escaped command\n---\nbody",
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"safe@catalog":[{"installPath":plugin}]}}).to_string(),
    );

    let result = discover(&home, &cwd);
    let tokens = result
        .global
        .iter()
        .flat_map(|entry| entry.forms.iter().map(|form| form.token.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(tokens, ["/safe:inside"]);
    assert!(result.completeness.is_complete());
}

#[test]
fn cancellation_returns_retained_results_with_an_exact_incomplete_reason() {
    struct Cancelled;

    impl crate::ports::invocation::InvocationCancellation for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let result = FilesystemInvocationCatalog::with_home_and_cancellation(
        Some(home.path().to_owned()),
        Arc::new(Cancelled),
    )
    .discover(InvocationDiscoveryRequest {
        generation: 9,
        cwd: cwd.path().to_owned(),
    });

    assert!(result.global.is_empty());
    assert!(result.project.is_empty());
    assert!(matches!(
        result.completeness.reasons(),
        [InvocationIncompleteReason::Cancelled {
            stage: crate::ports::invocation::InvocationDiscoveryStage::Filesystem
        }]
    ));
}
