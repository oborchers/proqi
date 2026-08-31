//! Linux CI parity runner for macOS and other Docker hosts.

use std::{fs, path::Path, process::Command};

const IMAGE: &str = "proqi-ci-linux:local";
const CONTEXT: &str = "tools/ci-linux";
const IMAGE_CONFIG: &str = "tools/ci-linux/image.json";
const HOST_RUNNER: &str = "tools/ci-linux/host-run.sh";

pub(super) fn run_local(root: &Path) -> Result<(), String> {
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
    run_container(
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

pub(super) fn run_prebuilt(
    root: &Path,
    image: &str,
    platform: Option<&str>,
    mode: &str,
) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize workspace: {error}"))?;
    validate_image(&root, image)?;
    let cache = root.join("target/ci-linux-cache");
    fs::create_dir_all(&cache).map_err(|error| format!("create Linux CI cache: {error}"))?;
    let source_mount = bind_mount(&root, "/source", true)?;
    let cache_mount = bind_mount(&cache, "/cache", false)?;
    let mut arguments = vec!["--rm".to_owned()];
    if let Some(platform) = platform {
        arguments.extend(["--platform".to_owned(), platform.to_owned()]);
    }
    arguments.extend([
        "--mount".to_owned(),
        source_mount,
        "--mount".to_owned(),
        cache_mount,
        "--tmpfs".to_owned(),
        "/work:exec,mode=1777".to_owned(),
        image.to_owned(),
        mode.to_owned(),
    ]);
    run_container(&root, arguments, "run prebuilt Linux CI image")?;
    if mode == "parity" {
        let artifacts = cache.join("target/package");
        super::debian_container::verify(
            &root,
            &artifacts.join("package/proqi-x86_64-unknown-linux-gnu.tar.gz"),
            &artifacts.join("debian-package/proqi_amd64.deb"),
        )?;
    }
    Ok(())
}

fn validate_image(root: &Path, image: &str) -> Result<(), String> {
    let path = root.join(IMAGE_CONFIG);
    let config: serde_json::Value = serde_json::from_slice(
        &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("parse {}: {error}", path.display()))?;
    if config
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err("Linux CI image config requires schema_version 1".to_owned());
    }
    let repository = config
        .get("repository")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Linux CI image config has no repository".to_owned())?;
    let digest = image
        .strip_prefix(&format!("{repository}@sha256:"))
        .ok_or_else(|| format!("Linux CI image must use {repository} by immutable digest"))?;
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(())
        .ok_or_else(|| "Linux CI image digest must contain 64 hexadecimal characters".to_owned())
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

fn run_container<I, S>(root: &Path, arguments: I, operation: &str) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let name = format!("proqi-ci-linux-{}", std::process::id());
    let status = Command::new("sh")
        .arg(root.join(HOST_RUNNER))
        .arg(name)
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

fn run_arguments<'a>(source: &'a str, cache: &'a str) -> [&'a str; 10] {
    [
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

    use super::{CONTEXT, IMAGE, bind_mount, build_arguments, run_arguments, validate_image};

    const DOCKERFILE: &str = include_str!("../../tools/ci-linux/Dockerfile");
    const HOST_RUNNER_SCRIPT: &str = include_str!("../../tools/ci-linux/host-run.sh");
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
        assert!(HOST_RUNNER_SCRIPT.contains("trap cleanup EXIT"));
        assert!(HOST_RUNNER_SCRIPT.contains("trap '' HUP INT TERM"));
        assert!(HOST_RUNNER_SCRIPT.contains("while kill -0 \"$client_pid\""));
        assert!(HOST_RUNNER_SCRIPT.contains("docker rm --force \"$name\""));

        for contract in [
            "rust:1.98.0-bookworm@sha256:",
            "rustup toolchain install 1.88.0",
            "cargo-nextest --version 0.9.143",
            "cargo-llvm-cov --version 0.9.0",
            "cargo-deny --version 0.20.2",
            "cargo-audit --version 0.22.2",
            "cargo-shear --version 1.11.2",
            "cargo-about --version 0.9.2",
            "NFPM_AMD64_SHA256=0660ca602b2d2d2ae4781a06c692b3eeb9d437ff",
            "NFPM_ARM64_SHA256=1c0f5f2999b9a974bfb04fdb0cc3306096de530a",
        ] {
            assert!(DOCKERFILE.contains(contract), "missing {contract}");
        }
        for command in [
            "cargo xtask quality",
            "cargo xtask test",
            "cargo +1.88.0 xtask msrv-full",
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

    #[test]
    fn public_image_requires_the_canonical_repository_and_an_immutable_digest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace");
        let repository = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../tools/ci-linux/image.json"
        ))
        .expect("image config")["repository"]
            .as_str()
            .expect("repository")
            .to_owned();
        assert!(validate_image(root, &format!("{repository}@sha256:{}", "a".repeat(64))).is_ok());
        assert!(validate_image(root, &format!("{repository}:latest")).is_err());
        assert!(validate_image(root, "ghcr.io/example/wrong@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
    }
}
