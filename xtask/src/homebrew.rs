//! Immutable prebuilt Homebrew formula rendering.

use std::{collections::BTreeMap, fs, path::Path};

use semver::Version;

const TARGETS: [&str; 3] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
];
const PLACEHOLDER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub(super) fn generate(root: &Path, artifacts: &Path, output: &Path) -> Result<(), String> {
    let artifacts = resolve(root, artifacts);
    let output = resolve(root, output);
    let checksums = load_checksums(&artifacts)?;
    let version = super::release::workspace_version(root)?;
    write_formula(&output, &version, &checksums, false)
}

pub(super) fn write_rehearsal(
    output: &Path,
    version: &Version,
    host_archive: &str,
    host_digest: &str,
) -> Result<(), String> {
    let mut checksums = TARGETS
        .iter()
        .map(|target| ((*target).to_owned(), PLACEHOLDER.to_owned()))
        .collect::<BTreeMap<_, _>>();
    if let Some(target) = TARGETS
        .iter()
        .find(|target| host_archive == archive_name(target))
    {
        checksums.insert((*target).to_owned(), host_digest.to_owned());
    }
    write_formula(
        &output.join("proqi.rb.rehearsal"),
        version,
        &checksums,
        true,
    )
}

fn load_checksums(artifacts: &Path) -> Result<BTreeMap<String, String>, String> {
    TARGETS
        .iter()
        .map(|target| {
            let archive = archive_name(target);
            let checksum_file = artifacts.join(format!("{archive}.sha256"));
            let contents = fs::read_to_string(&checksum_file)
                .map_err(|error| format!("read {}: {error}", checksum_file.display()))?;
            let digest = parse_checksum(&contents, &archive)?;
            Ok(((*target).to_owned(), digest))
        })
        .collect()
}

fn parse_checksum(contents: &str, expected_name: &str) -> Result<String, String> {
    let mut fields = contents.split_whitespace();
    let digest = fields
        .next()
        .ok_or_else(|| format!("checksum for `{expected_name}` is empty"))?;
    let name = fields
        .next()
        .ok_or_else(|| format!("checksum for `{expected_name}` has no filename"))?;
    if fields.next().is_some()
        || name.trim_start_matches('*') != expected_name
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!("invalid checksum record for `{expected_name}`"));
    }
    Ok(digest.to_ascii_lowercase())
}

fn write_formula(
    output: &Path,
    version: &Version,
    checksums: &BTreeMap<String, String>,
    rehearsal: bool,
) -> Result<(), String> {
    let arm = checksum(checksums, TARGETS[0])?;
    let intel = checksum(checksums, TARGETS[1])?;
    let linux = checksum(checksums, TARGETS[2])?;
    let warning = if rehearsal {
        "# Rehearsal only. CI replaces zero checksums with verified target digests.\n"
    } else {
        ""
    };
    let formula = format!(
        r##"{warning}class Proqi < Formula
  desc "An agent-optimized terminal scratchpad for follow-up prompts"
  homepage "https://github.com/oborchers/proqi"
  version "{version}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "{base}/proqi-aarch64-apple-darwin.tar.gz"
      sha256 "{arm}"
    else
      url "{base}/proqi-x86_64-apple-darwin.tar.gz"
      sha256 "{intel}"
    end
  end

  on_linux do
    on_intel do
      url "{base}/proqi-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{linux}"
    end
  end

  def install
    bin.install "proqi"
    bin.install "proqi-installation.json"
    bash_completion.install "completions/proqi.bash" => "proqi"
    zsh_completion.install "completions/_proqi"
    fish_completion.install "completions/proqi.fish"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/proqi --version")
    assert_match "schema_version", shell_output("#{{bin}}/proqi capabilities --json")
  end
end
"##,
        base = format!("https://github.com/oborchers/proqi/releases/download/v{version}")
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create formula directory: {error}"))?;
    }
    fs::write(output, formula).map_err(|error| format!("write {}: {error}", output.display()))
}

fn checksum<'a>(checksums: &'a BTreeMap<String, String>, target: &str) -> Result<&'a str, String> {
    checksums
        .get(target)
        .map(String::as_str)
        .ok_or_else(|| format!("missing checksum for `{target}`"))
}

fn archive_name(target: &str) -> String {
    format!("proqi-{target}.tar.gz")
}

fn resolve(root: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{TARGETS, archive_name, generate, parse_checksum};
    use std::fs;

    #[test]
    fn checksum_parser_requires_exact_archive_identity() {
        let name = archive_name(TARGETS[0]);
        let valid = format!("{}  {name}\n", "a".repeat(64));
        assert_eq!(parse_checksum(&valid, &name), Ok("a".repeat(64)));
        assert!(parse_checksum(&valid, &archive_name(TARGETS[1])).is_err());
        assert!(parse_checksum("xyz  file\n", "file").is_err());
    }

    #[test]
    fn formula_uses_immutable_assets_and_no_update_hook() {
        let root = tempfile::tempdir().expect("temporary root");
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .expect("workspace manifest");
        let artifacts = root.path().join("artifacts");
        fs::create_dir(&artifacts).expect("artifacts directory");
        for (index, target) in TARGETS.iter().enumerate() {
            let name = archive_name(target);
            fs::write(
                artifacts.join(format!("{name}.sha256")),
                format!("{}  {name}\n", format!("{index:x}").repeat(64)),
            )
            .expect("checksum fixture");
        }
        let formula = root.path().join("Formula/proqi.rb");
        generate(root.path(), &artifacts, &formula).expect("formula generation");
        let contents = fs::read_to_string(formula).expect("formula");
        assert!(contents.contains("releases/download/v0.1.0"));
        assert!(contents.contains("bin.install \"proqi-installation.json\""));
        assert!(!contents.contains("post_install"));
        assert!(!contents.contains("system \"brew\""));
    }
}
