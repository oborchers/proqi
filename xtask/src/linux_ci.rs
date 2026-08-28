//! Linux CI parity runner for macOS and other Docker hosts.

use std::{fs, path::Path, process::Command};

const IMAGE: &str = "proqi-ci-linux:local";
const CONTEXT: &str = "tools/ci-linux";

pub(super) fn run(root: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize workspace: {error}"))?;
    let cache = root.join("target/ci-linux-cache");
    fs::create_dir_all(&cache).map_err(|error| format!("create Linux CI cache: {error}"))?;
    let context = root.join(CONTEXT);
    let context = context
        .to_str()
        .ok_or_else(|| format!("Docker build context is not UTF-8: {}", context.display()))?;

    println!("+ Build pinned Docker Linux CI image");
    run_docker(&root, build_arguments(context), "build Linux CI image")?;

    let source_mount = bind_mount(&root, "/source", true)?;
    let cache_mount = bind_mount(&cache, "/cache", false)?;
    println!("+ Run Docker Linux CI parity");
    run_docker(
        &root,
        run_arguments(&source_mount, &cache_mount),
        "run Linux CI parity",
    )?;

    let artifacts = cache.join("target/package");
    super::debian_container::verify(
        &root,
        &artifacts.join("package/proqi-x86_64-unknown-linux-gnu.tar.gz"),
        &artifacts.join("debian-package/proqi_amd64.deb"),
    )
}

fn run_docker<const N: usize>(
    root: &Path,
    arguments: [&str; N],
    operation: &str,
) -> Result<(), String> {
    let status = Command::new("docker")
        .args(arguments)
        .current_dir(root)
        .status()
        .map_err(|error| format!("{operation}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{operation} exited with {status}"))
}

fn bind_mount(path: &Path, destination: &str, readonly: bool) -> Result<String, String> {
    let source = path
        .to_str()
        .ok_or_else(|| format!("Docker mount path is not UTF-8: {}", path.display()))?;
    if source.contains(',') {
        return Err(format!(
            "Docker mount path contains an unsupported comma: {}",
            path.display()
        ));
    }
    let readonly = if readonly { ",readonly" } else { "" };
    Ok(format!(
        "type=bind,src={source},dst={destination}{readonly}"
    ))
}

fn build_arguments(context: &str) -> [&str; 6] {
    [
        "build",
        "--platform",
        "linux/amd64",
        "--tag",
        IMAGE,
        context,
    ]
}

fn run_arguments<'a>(source: &'a str, cache: &'a str) -> [&'a str; 11] {
    [
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "--mount",
        source,
        "--mount",
        cache,
        "--tmpfs",
        "/work:exec,mode=1777",
        IMAGE,
    ]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CONTEXT, IMAGE, bind_mount, build_arguments, run_arguments};

    const DOCKERFILE: &str = include_str!("../../tools/ci-linux/Dockerfile");
    const RUNNER: &str = include_str!("../../tools/ci-linux/run.sh");

    #[test]
    fn parity_runner_builds_a_local_image_and_mounts_source_read_only() {
        let build = build_arguments(CONTEXT);
        assert!(build.contains(&"linux/amd64"));
        assert!(build.contains(&IMAGE));
        assert!(build.contains(&CONTEXT));

        let source = bind_mount(Path::new("/tmp/proqi"), "/source", true).expect("source");
        let cache = bind_mount(Path::new("/tmp/cache"), "/cache", false).expect("cache");
        let run = run_arguments(&source, &cache);
        assert!(run.contains(&"linux/amd64"));
        assert!(run.contains(&IMAGE));
        assert!(source.ends_with(",readonly"));
        assert!(!cache.ends_with(",readonly"));

        for contract in [
            "rust:1.98.0-bookworm@sha256:",
            "rustup toolchain install 1.88.0",
            "cargo-nextest --version 0.9.143",
            "cargo-llvm-cov --version 0.9.0",
            "cargo-deny --version 0.20.2",
            "cargo-audit --version 0.22.2",
            "cargo-shear --version 1.11.2",
            "cargo-about --version 0.9.2",
            "NFPM_SHA256=0660ca602b2d2d2ae4781a06c692b3eeb9d437ff",
        ] {
            assert!(DOCKERFILE.contains(contract), "missing {contract}");
        }
        for command in [
            "cargo xtask check",
            "cargo xtask test",
            "cargo +1.88.0 xtask msrv",
            "cargo xtask audit",
            "cargo xtask coverage",
            "cargo xtask package",
            "cargo xtask crate-package",
            "cargo xtask debian-package",
        ] {
            assert!(RUNNER.contains(command), "missing {command}");
        }
        for cache in [
            "use_target stable",
            "use_target msrv",
            "use_target coverage",
            "use_target package",
        ] {
            assert!(RUNNER.contains(cache), "missing {cache}");
        }
    }

    #[test]
    fn comma_in_a_mount_path_fails_before_docker() {
        let error = bind_mount(Path::new("/tmp/proqi,copy"), "/source", true)
            .expect_err("comma must fail closed");
        assert!(error.contains("unsupported comma"));
    }
}
