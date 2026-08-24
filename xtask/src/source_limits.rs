//! Repository source ownership and physical line limits.

use ignore::WalkBuilder;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_SOURCE_LINES: usize = 500;
const SOURCE_EXTENSIONS: &[&str] = &[
    "astro", "cjs", "css", "cts", "html", "js", "jsx", "less", "mjs", "mts", "rs", "scss",
    "svelte", "ts", "tsx", "vue",
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let tracked = tracked_files(root)?;
    let files = collect_source_files(root, &tracked)?;
    if files.is_empty() {
        return Err("source limit scan found no first-party source files".to_owned());
    }

    let mut violations = Vec::new();
    for path in files {
        let source = read_repository_source(root, &path)?;
        let line_count = source.lines().count();
        if line_count > MAX_SOURCE_LINES {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            violations.push(format!("{}: {line_count} lines", relative.display()));
        }
    }

    if violations.is_empty() {
        println!(
            "source limits: every first-party source file is at most {MAX_SOURCE_LINES} lines"
        );
        Ok(())
    } else {
        Err(format!(
            "first-party source files exceed the {MAX_SOURCE_LINES}-line limit:\n{}",
            violations.join("\n")
        ))
    }
}

fn tracked_files(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let output = Command::new("git")
        .args(["ls-files", "--cached", "-z"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git ls-files: {error}"))?;
    if !output.status.success() {
        return Err(format!("git ls-files exited with {}", output.status));
    }
    let paths = String::from_utf8(output.stdout)
        .map_err(|error| format!("git ls-files returned a non-UTF-8 path: {error}"))?;
    Ok(paths
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn collect_source_files(
    root: &Path,
    tracked: &BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>, String> {
    let mut files = tracked
        .iter()
        .filter(|path| is_source_file(path))
        .map(|path| root.join(path))
        .filter(|path| path.exists() || path.is_symlink())
        .collect::<BTreeSet<_>>();

    let root_copy = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .require_git(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(false)
        .sort_by_file_path(Ord::cmp)
        .filter_entry(move |entry| {
            entry.path() == root_copy || entry.file_name() != OsStr::new(".git")
        });

    for result in builder.build() {
        let entry = result.map_err(|error| format!("walk repository sources: {error}"))?;
        if entry
            .file_type()
            .is_some_and(|kind| kind.is_file() || kind.is_symlink())
            && is_source_file(entry.path())
        {
            files.insert(entry.into_path());
        }
    }
    Ok(files)
}

fn read_repository_source(root: &Path, path: &Path) -> Result<String, String> {
    if path.is_symlink() {
        let canonical_root = fs::canonicalize(root)
            .map_err(|error| format!("resolve repository root {}: {error}", root.display()))?;
        let target = fs::canonicalize(path)
            .map_err(|error| format!("resolve source symlink {}: {error}", path.display()))?;
        if !target.starts_with(&canonical_root) {
            return Err(format!(
                "source symlink escapes repository: {} -> {}",
                path.display(),
                target.display()
            ));
        }
    }
    fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn source_policy_covers_rust_and_common_frontend_languages() {
        for path in [
            "src/lib.rs",
            "ui/view.tsx",
            "ui/theme.css",
            "ui/page.svelte",
            "ui/component.vue",
            "ui/page.astro",
        ] {
            assert!(is_source_file(Path::new(path)), "uncovered source: {path}");
        }
        assert!(!is_source_file(Path::new("PRODUCT.md")));
    }

    #[test]
    fn configured_ceiling_is_inclusive() {
        assert_eq!("line\n".repeat(MAX_SOURCE_LINES).lines().count(), 500);
        assert_eq!("line\n".repeat(MAX_SOURCE_LINES + 1).lines().count(), 501);
    }

    #[test]
    fn ignored_untracked_source_is_excluded_but_tracked_source_is_not() {
        let fixture = Fixture::new();
        fixture.write(".gitignore", "generated/\n");
        fixture.write("generated/ignored.rs", "ignored\n");
        let path = PathBuf::from("generated/ignored.rs");

        let untracked = collect_source_files(fixture.path(), &BTreeSet::new());
        assert!(
            !untracked
                .expect("collect untracked sources")
                .contains(&fixture.path().join(&path))
        );

        let tracked = BTreeSet::from([path.clone()]);
        let sources = collect_source_files(fixture.path(), &tracked);
        assert!(
            sources
                .expect("collect tracked sources")
                .contains(&fixture.path().join(path))
        );
    }

    #[test]
    fn nonignored_untracked_source_is_included() {
        let fixture = Fixture::new();
        fixture.write("scratch/component.tsx", "export const value = 1;\n");
        let sources = collect_source_files(fixture.path(), &BTreeSet::new());
        assert!(
            sources
                .expect("collect sources")
                .contains(&fixture.path().join("scratch/component.tsx"))
        );
    }

    #[test]
    fn nested_gitignore_and_negation_are_respected() {
        let fixture = Fixture::new();
        fixture.write("ui/.gitignore", "*.ts\n!keep.ts\n");
        fixture.write("ui/ignored.ts", "ignored\n");
        fixture.write("ui/keep.ts", "kept\n");
        let sources = collect_source_files(fixture.path(), &BTreeSet::new())
            .expect("collect sources with nested ignore");
        assert!(!sources.contains(&fixture.path().join("ui/ignored.ts")));
        assert!(sources.contains(&fixture.path().join("ui/keep.ts")));
    }

    #[cfg(unix)]
    #[test]
    fn source_symlink_cannot_escape_repository() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let outside = TempDir::new().expect("create outside fixture");
        let outside_source = outside.path().join("outside.rs");
        fs::write(&outside_source, "outside\n").expect("write outside source");
        let link = fixture.path().join("escape.rs");
        symlink(&outside_source, &link).expect("create source symlink");

        let error = read_repository_source(fixture.path(), &link)
            .expect_err("escaping source symlink must fail");
        assert!(error.contains("source symlink escapes repository"));
    }

    struct Fixture {
        directory: TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                directory: TempDir::new().expect("create fixture"),
            }
        }

        fn path(&self) -> &Path {
            self.directory.path()
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.path().join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create fixture parent");
            }
            let mut file = fs::File::create(path).expect("create fixture file");
            file.write_all(content.as_bytes())
                .expect("write fixture file");
        }
    }
}
