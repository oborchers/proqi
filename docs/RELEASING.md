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
- A `release` environment accepts only `v*.*.*` tags and records the
  publication job without a manual approval gate. The release-plan validation
  remains authoritative for canonical stable semantic versions.
- crates.io trusts only `oborchers/proqi`, `release.yml`, and the `release`
  environment through GitHub OIDC. No long-lived registry token is available
  to GitHub Actions.
- The Homebrew formula is generated only from the verified release checksums.

## Fast local release preparation

Routine preparation validates reviewed release inputs and repository hygiene:

```shell
cargo xtask release-plan <vX.Y.Z>
git diff --check
```

`release-plan` checks the Cargo version, canonical absent tag, matching release
notes, bounded reviewed `release-highlights.json`, exact locally known `main`
identity, and a clean worktree. It does not compile, package, run containers, or
repeat CI. Run Actionlint and Zizmor when workflow files changed.

The complete development and milestone gates remain available. Use the gates
relevant to the changed boundary, then use all of them for pipeline changes:

```shell
cargo xtask check
cargo xtask test-pty
cargo xtask audit
cargo xtask package
cargo xtask crate-package
cargo xtask release-rehearsal
cargo xtask ci-linux-smoke <image-repository@sha256:digest>
cargo xtask ci-linux-amd64 <image-repository@sha256:digest>
actionlint .github/workflows/*.yml
zizmor --pedantic .github/workflows/*.yml
```

Coverage remains one relevant-change CI gate. Full Rust 1.88 testing is selected
in ordinary CI for dependency, toolchain, manifest, packaging, and workflow
changes, and is manually available through `Full MSRV diagnostic`. The explicit
amd64 container command is diagnostic because it can use emulation on a non-x86
host. Routine release preparation runs neither container path.

`cargo xtask crate-package` runs `cargo package --locked` and `cargo publish
--dry-run --locked` without a token. It checks the exact crate member allowlist,
normalized manifest, clean VCS metadata, registry-only dependencies, private
markers, checksum, isolated packaged-source installation, version,
capabilities, and disposable state. Ordinary CI owns this dry run once in
`Registry package contract`. The release-ready candidate uses
`cargo xtask crate-evidence`, which performs the necessary locked packaging and
installed-source checks without repeating the publication dry run.

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

## Public Linux QA tools image

`tools/ci-linux/image.json` exclusively owns the GHCR repository name. The
`Linux QA tools image` workflow builds amd64 and arm64 on native hosted runners.
Pull requests build without pushing. Trusted main runs use only `GITHUB_TOKEN`
with `contents: read` and job-scoped `packages: write`, publish content-derived
tags, attach BuildKit provenance and SBOMs, and create the multi-architecture
manifest. Registry cache tags are disposable acceleration, never evidence.

Consumers copy the published manifest digest from the successful workflow and
pass the full `repository@sha256:digest` reference to `ci-linux-smoke` or
`ci-linux-amd64`. The xtask rejects mutable tags and any other repository. Do
not use `latest`. After the first trusted main publication, verify that the GHCR
package is public and linked to this public repository. GitHub creates a new
container package as private, so an owner must change its visibility once in
the package settings and rerun the manual recovery workflow. The publish job
logs out and requires anonymous access to the exact manifest digest, so it
fails closed until that one-time prerequisite is complete.

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
2. Enable dependency alerts, grouped Dependabot security updates, secret
   scanning, and push protection.
3. Keep Issues enabled and Discussions disabled.
4. Require the aggregate `Required CI result` check on `main` while allowing
   the repository owner to push directly.
5. Protect `v*` tags from deletion, non-fast-forward changes, and unauthorized
   creation.
6. Configure the `release` environment without required reviewers. Restrict its
   deployment branch and tag policy to the repository's stable release tags.
7. Upload `assets/proqi-social-preview.png` through the repository social
   preview setting.
8. Verify the default branch, visibility, MIT detection, contribution guide,
   security policy, Code of Conduct, description, topics, Issues state, and
   Discussions state from the public repository view.

## CI rehearsal and release

After the readiness audit:

1. Prepare and review the Cargo version, `.github/release-notes/vX.Y.Z.md`, and
   `release-highlights.json`, then push `main`.
2. The `Release candidate` workflow classifies that exact main SHA from the
   checked-in inputs. If release-ready, it builds the native candidates in
   parallel with ordinary CI and records one 30-day immutable candidate. It has
   no publication credentials.
3. Wait for `Required CI result` and the candidate workflow to pass on the exact
   release commit.
4. Create the exact annotated stable tag at that commit and push it. The tag is
   the single authorization to publish the crate, GitHub Release, and Homebrew
   formula. Do not create an empty public GitHub Release by hand.
5. The tag-triggered `Release` workflow requires the tag commit to be the exact
   prepared main SHA and finds exactly one successful, unexpired candidate for
   the same version and SHA. Missing, expired, duplicate, failed, or mismatched
   candidates fail closed. It downloads by REST artifact ID and checks the
   artifact archive digest before extraction.
6. The promotion job consumes those already-built bytes. It verifies every
   byte and attestation, creates a GitHub Release draft, publishes the exact
   crate through crates.io trusted publishing, installs and tests the registry
   version, makes the Release public, downloads every public asset again, and
   requires byte identity with the candidate.
7. Only after public-byte verification does the workflow send the scoped
   `proqi_release_published` event. The tap verifies and tests the formula before
   committing it. No polling job is involved.
8. If the candidate is absent or expired, manually dispatch `Release candidate`
   at `main`, or at the exact protected tag for recovery. This path never
   publishes. Rerun promotion only after the exact candidate succeeds.
9. Verify the published GitHub Release, crates.io version, and Homebrew formula.

The release workflow never cancels an in-progress tag release. Any failed
target, smoke test, checksum, SBOM, attestation, or formula generation blocks
publication. Routine release work therefore ends at reviewed release metadata
and the protected tag. Every distribution step after that boundary is
automatic and fail-closed.

## crates.io publication boundary

The `proqi` crate configures one crates.io trusted publisher:

```text
Repository owner: oborchers
Repository:       proqi
Workflow:         release.yml
Environment:      release
```

The protected release job requests GitHub OIDC identity only after the exact
candidate and tag have passed validation. The pinned official crates.io action
exchanges that identity for a short-lived token and revokes it when the job
ends. No long-lived Cargo token is stored in GitHub, Cargo credentials, the
repository, workflow artifacts, logs, diagnostics, or local release state.

Before publishing, the workflow requires the Cargo version, tag, candidate
evidence, locally reproduced `.crate`, and SHA-256 digest to agree. It creates
the GitHub Release as a draft, then runs `cargo publish --locked`. The public
registry archive must match the candidate digest before the workflow installs
the exact registry version into fresh Cargo state and exercises its versioned
JSON contract. Only then may the GitHub Release become public and notify the
Homebrew tap.

Promotion is idempotent. When the registry version already exists, the workflow
downloads it and requires byte identity with the reviewed candidate rather than
publishing again. This also recovers when Cargo reports a false-negative after
an accepted upload. A missing, yanked, mismatched, or unverifiable registry
version fails closed. The GitHub Release remains a draft and Homebrew receives
no event.

If crates.io succeeds and a later GitHub step fails, rerun the same tag workflow.
It verifies the immutable registry bytes and resumes publication. If immutable
bytes or versions cannot align, recover through a new semantic version rather
than overwriting, deleting, or retagging public artifacts.

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

Dependabot version updates across Cargo, GitHub Actions, and the Rust toolchain
arrive in one weekly pull request. Cargo security updates are grouped separately
and opened immediately. Neither class is merged automatically: dependency diffs,
lockfile changes, release notes, checks, and provenance remain human-reviewed.

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
explicit `proqi update check --json` sees the released Cargo version. Before
upgrading the prior Homebrew installation, launch it with isolated state and
verify that an ordinary interactive startup offers the newly published formula
without an explicit check.

Ordinary uninstall must leave user data intact:

```shell
brew uninstall proqi
```
