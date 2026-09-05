//! Executable repository architecture policy.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const INNER_FORBIDDEN: &[&str] = &[
    "crate::adapters",
    "crate::ui",
    "crossterm",
    "ratatui",
    "rusqlite",
    "std::env",
    "std::fs",
    "env::",
    "fs::",
    "process::Command",
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let source_root = root.join("src");
    let mut files = Vec::new();
    collect_rust_files(&source_root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err("architecture scan found no Rust source files".to_owned());
    }

    let mut layer_counts = [0_usize; 6];
    let mut violations = Vec::new();
    for path in &files {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let source = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", relative.display()))?;
        classify(relative, &mut layer_counts);
        violations.extend(check_source(relative, &source));
        violations.extend(crate::shortcut_architecture::check_source(
            relative, &source,
        ));
    }
    violations.extend(crate::shortcut_architecture::required_owner_findings(root));
    violations.extend(crate::instructions::check(root)?);
    violations.extend(crate::herdr_compatibility::policy_findings(root)?);
    violations.extend(crate::release_policy::check(root)?);
    if layer_counts.contains(&0) {
        return Err(format!(
            "architecture scan was incomplete: domain={}, application={}, ports={}, adapters={}, ui={}, cli={}",
            layer_counts[0],
            layer_counts[1],
            layer_counts[2],
            layer_counts[3],
            layer_counts[4],
            layer_counts[5]
        ));
    }
    if violations.is_empty() {
        println!(
            "architecture policy: checked {} Rust source files",
            files.len()
        );
        Ok(())
    } else {
        violations.sort();
        Err(format!(
            "architecture policy violations:\n{}",
            violations.join("\n")
        ))
    }
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read directory {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| format!("read file type for {}: {error}", path.display()))?;
        if kind.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension() == Some(OsStr::new("rs"))
            && (kind.is_file() || kind.is_symlink())
        {
            files.push(path);
        }
    }
    Ok(())
}

fn classify(path: &Path, counts: &mut [usize; 6]) {
    let path = slash_path(path);
    if path.starts_with("src/domain/") {
        counts[0] += 1;
    } else if path.starts_with("src/application/") {
        counts[1] += 1;
    } else if path.starts_with("src/ports/") {
        counts[2] += 1;
    } else if path.starts_with("src/adapters/") {
        counts[3] += 1;
    } else if path.starts_with("src/ui/") {
        counts[4] += 1;
    } else if path.starts_with("src/cli/") {
        counts[5] += 1;
    }
}

fn check_source(path: &Path, source: &str) -> Vec<String> {
    let path_text = slash_path(path);
    if path_text.ends_with("/tests.rs") || path_text.contains("/tests/") {
        return Vec::new();
    }
    let normalized = normalized_paths(source).join("\n");
    let mut violations = Vec::new();
    if path_text.starts_with("src/domain/") {
        find_markers(
            path,
            &normalized,
            &["crate::application", "crate::ports"],
            &mut violations,
        );
        find_markers(path, &normalized, INNER_FORBIDDEN, &mut violations);
    } else if path_text.starts_with("src/application/") {
        find_markers(path, &normalized, INNER_FORBIDDEN, &mut violations);
    } else if path_text.starts_with("src/ports/") {
        find_markers(path, &normalized, INNER_FORBIDDEN, &mut violations);
        find_markers(path, &normalized, &["crate::application"], &mut violations);
    } else if path_text.starts_with("src/ui/") {
        find_markers(path, &normalized, &["crate::adapters"], &mut violations);
    }
    enforce_layer_edges(path, &normalized, &path_text, &mut violations);
    enforce_adapter_ownership(path, &normalized, &path_text, &mut violations);
    enforce_diagnostics_ownership(path, &normalized, &path_text, &mut violations);
    if path_text == "src/domain/mod.rs" && source.contains("pub mod ") {
        violations.push(format!(
            "{}: domain implementation modules must remain private",
            path.display()
        ));
    }
    violations
}

fn enforce_layer_edges(path: &Path, source: &str, path_text: &str, violations: &mut Vec<String>) {
    let layered = [
        "src/domain/",
        "src/ports/",
        "src/application/",
        "src/adapters/",
        "src/ui/",
    ]
    .iter()
    .any(|prefix| path_text.starts_with(prefix));
    if layered {
        find_markers(path, source, &["crate::cli"], violations);
    }
    if path_text.starts_with("src/adapters/") && !path_text.starts_with("src/adapters/terminal/") {
        find_markers(path, source, &["crate::ui"], violations);
    }
}

fn enforce_diagnostics_ownership(
    path: &Path,
    source: &str,
    path_text: &str,
    violations: &mut Vec<String>,
) {
    let owns_diagnostics = path_text == "src/adapters/diagnostics.rs"
        || path_text.starts_with("src/adapters/diagnostics/");
    if (source.contains("tracing::") || source.contains("tracing_subscriber")) && !owns_diagnostics
    {
        violations.push(format!(
            "{}: tracing is outside the typed diagnostics adapter",
            path.display()
        ));
    }
}

fn normalized_paths(source: &str) -> Vec<String> {
    use syn::visit::Visit as _;

    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    let mut visitor = DependencyVisitor { paths: Vec::new() };
    visitor.visit_file(&file);
    visitor.paths
}

struct DependencyVisitor {
    paths: Vec<String>,
}

impl<'ast> syn::visit::Visit<'ast> for DependencyVisitor {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        expand_use_tree(&item.tree, Vec::new(), &mut self.paths);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !has_test_configuration(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.paths.push(
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        );
        syn::visit::visit_path(self, path);
    }
}

fn expand_use_tree(tree: &syn::UseTree, mut prefix: Vec<String>, paths: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            expand_use_tree(&path.tree, prefix, paths);
        }
        syn::UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            paths.push(prefix.join("::"));
        }
        syn::UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            paths.push(prefix.join("::"));
        }
        syn::UseTree::Glob(_) => paths.push(format!("{}::*", prefix.join("::"))),
        syn::UseTree::Group(group) => {
            for nested in &group.items {
                expand_use_tree(nested, prefix.clone(), paths);
            }
        }
    }
}

fn has_test_configuration(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        matches!(
            &attribute.meta,
            syn::Meta::List(list)
                if list.path.is_ident("cfg") && list.tokens.to_string().contains("test")
        )
    })
}

fn enforce_adapter_ownership(
    path: &Path,
    source: &str,
    path_text: &str,
    violations: &mut Vec<String>,
) {
    for (marker, allowed) in [
        ("rusqlite", &["src/adapters/sqlite/"][..]),
        ("crossterm", &["src/adapters/terminal/", "src/bin/"][..]),
        (
            "ratatui",
            &["src/ui/", "src/adapters/terminal/", "src/bin/"][..],
        ),
        (
            "process::Command",
            &["src/adapters/process/", "src/bin/"][..],
        ),
        ("std::env", &["src/adapters/", "src/bin/"][..]),
        ("std::fs", &["src/adapters/", "src/bin/"][..]),
        ("env::", &["src/adapters/", "src/bin/"][..]),
        ("fs::", &["src/adapters/", "src/bin/"][..]),
    ] {
        if source.contains(marker) && !allowed.iter().any(|prefix| path_text.starts_with(prefix)) {
            violations.push(format!(
                "{}: `{marker}` is outside its owning adapter",
                path.display()
            ));
        }
    }
}

fn find_markers(path: &Path, source: &str, markers: &[&str], violations: &mut Vec<String>) {
    for marker in markers {
        if source.contains(marker) {
            violations.push(format!(
                "{}: forbidden dependency `{marker}`",
                path.display()
            ));
        }
    }
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_to_port_dependency_is_rejected() {
        let findings = check_source(
            Path::new("src/domain/model.rs"),
            "use crate::ports::editor::Editor;",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("crate::ports"));
    }

    #[test]
    fn owned_adapter_dependency_is_accepted() {
        assert!(
            check_source(
                Path::new("src/adapters/sqlite/mod.rs"),
                "use rusqlite::Connection;"
            )
            .is_empty()
        );
    }

    #[test]
    fn misplaced_adapter_dependency_is_rejected() {
        let findings = check_source(Path::new("src/cli/mod.rs"), "use rusqlite::Connection;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("owning adapter"));
    }

    #[test]
    fn direct_filesystem_access_is_rejected_outside_adapters() {
        let findings = check_source(Path::new("src/cli/mod.rs"), "use std::fs;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("owning adapter"));
    }

    #[test]
    fn public_domain_implementation_module_is_rejected() {
        let findings = check_source(Path::new("src/domain/mod.rs"), "pub mod model;");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("must remain private"));
    }

    #[test]
    fn grouped_application_to_adapter_import_is_rejected() {
        let findings = check_source(
            Path::new("src/application/service.rs"),
            "use crate::{adapters::sqlite::SqliteStore, domain::Session};",
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("crate::adapters"))
        );
    }

    #[test]
    fn tracing_outside_diagnostics_adapter_is_rejected() {
        let findings = check_source(
            Path::new("src/adapters/terminal/runner.rs"),
            "tracing::info!(event = \"raw\");",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("typed diagnostics adapter"));
    }

    #[test]
    fn tracing_inside_diagnostics_adapter_is_accepted() {
        assert!(
            check_source(
                Path::new("src/adapters/diagnostics.rs"),
                "tracing::info!(event = \"typed\");",
            )
            .is_empty()
        );
    }

    #[test]
    fn port_to_application_dependency_is_rejected() {
        let findings = check_source(
            Path::new("src/ports/store.rs"),
            "use crate::application::AppState;",
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("crate::application"))
        );
    }

    #[test]
    fn inner_layers_and_nonterminal_adapters_cannot_import_outer_composition() {
        let cli = check_source(
            Path::new("src/adapters/sqlite/load.rs"),
            "use crate::cli::RuntimeContext;",
        );
        assert!(cli.iter().any(|finding| finding.contains("crate::cli")));
        let ui = check_source(
            Path::new("src/adapters/sqlite/load.rs"),
            "use crate::ui::BoardApp;",
        );
        assert!(ui.iter().any(|finding| finding.contains("crate::ui")));
        assert!(
            check_source(
                Path::new("src/adapters/terminal/runner.rs"),
                "use crate::ui::BoardApp;",
            )
            .is_empty()
        );
    }

    #[test]
    fn test_only_adapter_fixture_is_accepted() {
        let source = "#[cfg(test)] mod tests { use crate::{adapters::memory::FakeClock}; }";
        assert!(check_source(Path::new("src/application/model.rs"), source).is_empty());
    }

    #[test]
    fn adjacent_test_modules_keep_the_same_test_only_dependency_boundary() {
        let source = "use crate::{adapters::memory::FakeClock}; use std::env;";
        assert!(check_source(Path::new("src/application/control/tests.rs"), source).is_empty());
    }
}
