# Releasing Proqi

This runbook describes the release boundary. It does not grant publication
authority by itself.

## Release invariants

- `Cargo.toml` is the only version source.
- Stable tags use exact `vX.Y.Z` syntax and must equal the Cargo version.
- Release artifacts exist only for Apple silicon macOS, Intel macOS, and
  x86-64 GNU Linux.
- The crates.io package is an installable binary package, is restricted to the
  `crates-io` registry, and does not define a supported Rust library API.
- The Debian asset is exactly `proqi_amd64.deb`, has Debian revision `-1`, and
  reuses the verified x86-64 GNU/Linux archive binary byte for byte.
- The GNU/Linux archive supports glibc 2.35 or newer. It is built on an Ubuntu
  22.04 native runner and must pass the repository-owned ELF symbol ceiling.
- Every archive includes the executable, MIT license, third-party notices,
  standalone installation marker, and Bash, Zsh, and Fish completions.
- Every archive and the Debian package have a SHA-256 file, SPDX 2.3 JSON SBOM,
  provenance attestation, and SBOM attestation.
- The release workflow has no dependency cache and every Action is pinned by
  full commit SHA.
- A `release` environment records and restricts the publication job without a
  manual approval gate.
- The Homebrew formula is generated only from the verified release checksums.

## Local release candidate gate

Install the developer tools reported by `cargo xtask setup`, plus `cargo-dist
0.32.0`, `cargo-about 0.9.2`, Syft 1.51.0, Actionlint, Zizmor, Ruby, Homebrew,
Asciinema, agg, Expect, and librsvg. Then run:

```shell
cargo xtask check
cargo xtask test-pty
cargo xtask coverage
cargo xtask audit
cargo xtask package
cargo xtask crate-package
cargo xtask verify-linux-archive target/package/proqi-x86_64-unknown-linux-gnu.tar.gz
cargo +1.88.0 xtask msrv
cargo xtask release-plan <vX.Y.Z>
cargo xtask release-rehearsal
actionlint .github/workflows/ci.yml .github/workflows/release-candidate.yml .github/workflows/release.yml
zizmor --pedantic .github/workflows/ci.yml .github/workflows/release-candidate.yml .github/workflows/release.yml
git diff --check
```

Inspect the rehearsal archive members, notices, checksum, SBOM, formula, social
preview, diagram, and README GIF. Rehearsal output remains ignored below
`target/release-rehearsal`.

`cargo xtask crate-package` runs `cargo package --locked` and `cargo publish
--dry-run --locked` without a token. It checks the exact crate member allowlist,
normalized manifest, clean VCS metadata, registry-only dependencies, private
markers, checksum, isolated packaged-source installation, version,
capabilities, and disposable state. Run it once with the checked-in toolchain
and once with Rust 1.88.

Debian assembly is authoritative only on native x86-64 GNU/Linux. The candidate
job runs:

```shell
cargo xtask debian-package \
  target/package/proqi-x86_64-unknown-linux-gnu.tar.gz \
  target/package
cargo xtask verify-debian \
  target/package/proqi-x86_64-unknown-linux-gnu.tar.gz \
  target/package/proqi_amd64.deb
```

The verifier inspects metadata, members, modes, absence of maintainer scripts,
dependency derivation, binary identity, and install, remove, state-preservation,
and reinstall behavior in pinned Ubuntu 22.04, Ubuntu 24.04, and Debian
bookworm containers.

## Repository settings

The public repository metadata is reviewed as one unit:

```text
Description: An agent-optimized terminal scratchpad for capturing, editing, and submitting follow-up prompts beside coding-agent sessions.
Website after first release: https://github.com/oborchers/proqi/releases/latest
Topics: rust, terminal, tui, cli, ratatui, developer-tools, ai-agents, coding-agents, prompt-management, scratchpad, local-first, sqlite, productivity, herdr
Social preview: assets/proqi-social-preview.png
```

The corresponding repository command is:

```shell
gh repo edit oborchers/proqi \
  --visibility public \
  --enable-issues \
  --enable-discussions=false \
  --description "An agent-optimized terminal scratchpad for capturing, editing, and submitting follow-up prompts beside coding-agent sessions." \
  --add-topic rust,terminal,tui,cli,ratatui,developer-tools,ai-agents,coding-agents,prompt-management,scratchpad,local-first,sqlite,productivity,herdr
```

After the first release exists, add the website separately:

```shell
gh repo edit oborchers/proqi \
  --homepage "https://github.com/oborchers/proqi/releases/latest"
```

Manual and API-managed settings must also:

1. Enable private vulnerability reporting.
2. Keep Issues enabled and Discussions disabled.
3. Require the aggregate `Required CI result` check on `main` while allowing
   the repository owner to push directly.
4. Protect `v*` tags from deletion, non-fast-forward changes, and unauthorized
   creation.
5. Configure the `release` environment without required reviewers. Restrict its
   deployment branch and tag policy to the repository's stable release tags.
6. Upload `assets/proqi-social-preview.png` through the repository social
   preview setting.
7. Verify the default branch, visibility, MIT detection, contribution guide,
   security policy, Code of Conduct, description, topics, Issues state, and
   Discussions state from the public repository view.

## CI rehearsal and release

After the readiness audit:

1. Re-enable CI and push the consolidated `main` branch once.
2. Wait for `Required CI result` to pass on Linux and macOS.
3. Close superseded dependency pull requests only after their equivalent
   versions are present on `main`.
4. Dispatch `Release candidate` from `main` with the intended stable tag. This
   performs the expensive hosted matrix exactly once and retains the immutable
   candidate for seven days. The Linux job builds on Ubuntu 22.04, enforces the
   `GLIBC_2.35` ceiling, starts the exact archive in pinned Ubuntu 22.04, Debian
   bookworm, and Ubuntu 24.04 images, then builds and tests the Debian package
   from that same binary. The credential-free crate job records the exact
   `.crate`, its checksum, and dry-run evidence without publishing it.
5. Inspect all three archives, the Debian package, checksums, SBOMs,
   attestations, formula, crate evidence, Debian evidence, candidate manifest,
   source commit, and artifact digest from that successful run.
6. Obtain explicit approval for the exact crates.io version and source commit.
   Publish only that reviewed version as described below, then verify a fresh
   registry installation. This approval does not authorize a GitHub tag.
7. Obtain separate explicit approval for the exact annotated stable tag and
   GitHub Release. Create and push the tag at the candidate commit. The
   tag-triggered `Release` workflow downloads and verifies the candidate, adds
   tag-bound attestations, and publishes the same archives and Debian bytes
   without rebuilding. The tag also authorizes the documented Homebrew wake-up.
8. If the candidate is absent or expired, delete the unpublished tag, dispatch
   a replacement candidate for the same tag and commit, then recreate the tag.
9. Verify the published Release and every downloaded asset. Publication sends
   a scoped `proqi_release_published` event to the public tap, which verifies
   and publishes the formula. An explicit manual dispatch remains available
   for recovery. The tap does not poll for releases.

The release workflow never cancels an in-progress tag release. Any failed
target, smoke test, checksum, SBOM, attestation, or formula generation blocks
draft creation.

## crates.io publication boundary

The Cargo API token is not available to CI and must not be written to the
repository, Cargo credentials, a temporary file, logs, diagnostics, shell
history, or another password store. Use the existing 1Password Cargo
Environment to expose `CARGO_REGISTRY_TOKEN` only to the one explicitly
approved publication process. Do not inspect or print the value.

Immediately before publication, confirm that the worktree is clean, `HEAD`
matches the reviewed candidate source, the Cargo version matches the intended
tag, the package name and version are available, and `cargo xtask
crate-package` still passes. The only authorized registry mutation is:

```shell
cargo publish --locked
```

After crates.io accepts the version, install the exact registry version into a
fresh disposable Cargo root and run `proqi --version`, `proqi capabilities
--json`, and one isolated state command. Confirm that crates.io renders the
README without broken local links. A published crate version is immutable and
must never be overwritten or republished.

Publication authorities remain independent:

- crates.io approval authorizes only `cargo publish --locked` for the reviewed
  package version.
- GitHub approval authorizes only the reviewed stable tag and Release; it also
  triggers the already documented Homebrew synchronization.
- The Homebrew tap verifies and publishes only after the GitHub Release exists.

If crates.io succeeds and GitHub fails, keep the registry version intact, fix
the GitHub path without rebuilding or changing bytes, and report the partial
release. If GitHub succeeds and crates.io fails, do not change the Release; fix
the registry problem only when the same version remains publishable. If
immutable bytes or versions can no longer align, recover through a new semantic
version rather than overwriting, deleting, or retagging public artifacts.

## Public Homebrew tap

The standard personal tap is:

```text
oborchers/homebrew-tap
└── Formula
    └── proqi.rb
```

Create it as a public repository only after the Proqi release assets exist. The
tap should contain the generated formula, MIT license, concise README, and a CI
workflow that runs formula syntax, style, audit, install, and `brew test` on
supported macOS. Formula URLs remain immutable and reference one exact Proqi
release tag.

Before pushing a formula update:

```shell
brew style Formula/proqi.rb
brew audit --strict --formula Formula/proqi.rb
brew install --formula ./Formula/proqi.rb
brew test proqi
proqi --version
```

The supported user commands are:

```shell
brew install oborchers/tap/proqi
brew upgrade --formula oborchers/tap/proqi
```

Ongoing formula synchronization is owned by `oborchers/homebrew-tap`. A GitHub
App installed only on that repository sends a wake-up event after publication.
The app has only `Contents: write`, its installation token is short-lived, and
its private key is confined to Proqi's protected `release` environment. The tap
then uses its own short-lived `GITHUB_TOKEN` to verify the latest stable Proqi
release before committing one exact formula update. An explicit manual dispatch
remains available for recovery. No periodic release check runs, and no personal
access token is stored in either repository.

Homebrew Core, bottles, casks, signing, and notarization are outside the current
release.

## Verification after publication

Download every archive from the Release rather than reusing workflow output:

```shell
shasum -a 256 -c proqi-aarch64-apple-darwin.tar.gz.sha256
gh attestation verify proqi-aarch64-apple-darwin.tar.gz \
  --repo oborchers/proqi \
  --signer-workflow github.com/oborchers/proqi/.github/workflows/release.yml
```

Repeat for Intel macOS, x86-64 GNU Linux, and `proqi_amd64.deb`. Verify that
every SBOM attestation uses `https://spdx.dev/Document/v2.3`. Download the
Debian checksum from the Release, run `sha256sum --check`, and repeat the
container install, remove, state-preservation, and reinstall contract against
the public bytes.

Install the exact published crate version into a fresh Cargo root and verify
its version and JSON capabilities. Install through the public tap, run `brew
test proqi`, launch the TUI, create and resume one session, and verify an
explicit `proqi update check --json` sees the released Cargo version.

Ordinary uninstall must leave user data intact:

```shell
brew uninstall proqi
```
