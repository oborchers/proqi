# Releasing Proqi

This runbook describes the release boundary. It does not grant publication
authority by itself.

## Release invariants

- `Cargo.toml` is the only version source.
- Stable tags use exact `vX.Y.Z` syntax and must equal the Cargo version.
- Every `.github/release-notes/vX.Y.Z.md` has one exact matching version in
  `release-highlights.json`, with three to six jointly reviewed user-facing
  highlights. Release planning and package assembly fail when they diverge.
- Release archives exist only for the six targets printed by `cargo xtask
  release-targets triples`: Apple silicon and Intel macOS, x86-64 and ARM64 GNU
  Linux, and x86-64 and ARM64 musl Linux.
- The crates.io package is an installable binary package, is restricted to the
  `crates-io` registry, and does not define a supported Rust library API.
- Debian assets are exactly `proqi_amd64.deb` and `proqi_arm64.deb`, have Debian
  revision `-1`, and reuse their matching verified GNU/Linux archive binaries
  byte for byte.
- GNU/Linux archives support glibc 2.35 or newer, build on matching native
  Ubuntu 22.04 runners, and pass the repository-owned ELF symbol ceiling. The
  statically linked musl archives are the verified fallback for musl systems
  and glibc older than 2.35.
- Every archive includes the executable, MIT license, third-party notices,
  standalone installation marker, and Bash, Zsh, and Fish completions.
- Every archive, Debian package, and release-attached installer has a SHA-256
  file, SPDX 2.3 JSON SBOM, provenance attestation, and SBOM attestation.
- Promotion derives one release-wide SPDX document and exact subject checksum
  set from the typed primary-artifact registry, then binds both provenance and
  SBOM attestations to the protected tag without rebuilding candidate bytes.
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
cargo xtask check-full
cargo xtask test-pty
cargo xtask audit
cargo xtask package
cargo xtask installer-package target/installer
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

The candidate matrix adds three archive jobs and one small installer job to the
former three-archive topology. At 20 to 35 minutes per new archive job, the
expected added billed Linux work is about 60 to 105 runner-minutes per release
candidate, plus at most 10 runner-minutes for the installer. Parallel wall time
remains bounded by the existing 35-minute archive timeout. This is a
runner-minute estimate, not a currency estimate, because GitHub pricing and
repository allowances are external and may change.

### Target and dependency evidence

The six-target registry is the source for Cargo triples, runners, archive and
Debian names, libc policy, Homebrew inclusion, installer cases, candidate files,
and release documentation. Candidate jobs use native `ubuntu-22.04-arm` for
ARM64. Musl jobs use pinned Zig 0.14.1 and cargo-zigbuild 0.23.3, then require a
static ELF with no interpreter or `NEEDED` entries before runtime tests.

The resolved Linux dependency graph has no OpenSSL or system SQLite boundary.
`ureq` uses rustls with ring, and ring 0.17.14 supports both selected Linux
architectures through its `cc` build path. `rusqlite` enables `bundled`, so its
`libsqlite3-sys` source is compiled for the selected target instead of linking a
host SQLite. Clipboard support is `arboard` 3.6.1 with both X11 through `x11rb`
and Wayland data-control through `wl-clipboard-rs`; those selected paths do not
require a host `libxcb` or `libwayland` link. Final archive inspection and
native or matching-platform container startup remain the authority. A target
that fails either check is not promotable.

No new credential is required. Candidate builds retain read-only contents plus
the existing GitHub OIDC attestation permissions. Promotion retains the current
workflow token, existing crates.io trusted publishing exchange, and scoped
Homebrew tap notification app. Archives, installers, checksums, SBOMs, and
attestations remain GitHub Release bytes and claims.

`cargo xtask crate-package` runs `cargo package --locked` and `cargo publish
--dry-run --locked` without a token. It checks the exact crate member allowlist,
normalized manifest, clean VCS metadata, registry-only dependencies, private
markers, checksum, isolated packaged-source installation, version,
capabilities, and disposable state. Ordinary CI owns this dry run once in
`Registry package contract`. The release-ready candidate uses
`cargo xtask crate-evidence`, which performs the necessary locked packaging and
installed-source checks without repeating the publication dry run.

Debian assembly is authoritative on matching native GNU/Linux runners. For
each target emitted with Debian metadata by the registry, the candidate job
runs:

```shell
cargo xtask debian-package \
  target/package/proqi-x86_64-unknown-linux-gnu.tar.gz \
  target/package x86_64-unknown-linux-gnu
cargo xtask verify-debian \
  target/package/proqi-x86_64-unknown-linux-gnu.tar.gz \
  target/package/proqi_amd64.deb x86_64-unknown-linux-gnu
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
and run-qualified tags, attach BuildKit provenance and SBOMs, and create the
multi-architecture manifest. A publication attempt proves every intended tag
is absent before pushing and never overwrites it. Registry cache tags are
disposable acceleration, never evidence.

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

1. Prepare and jointly review the Cargo version,
   `.github/release-notes/vX.Y.Z.md`, and the exact matching bounded entry in
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
   candidates fail closed. Later main commits do not invalidate the protected
   tag because the successful main CI and candidate records bind the prepared
   SHA. It downloads by REST artifact ID and checks the artifact archive digest
   before extraction.
6. The promotion job consumes those already-built bytes. It verifies every
   byte and attestation, creates or resumes a GitHub Release draft, publishes
   the exact crate through crates.io trusted publishing, installs and tests the registry
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

Repeat for Intel macOS, both GNU Linux archives and Debian packages, both musl
archives, and `proqi-installer.sh`. Verify that
every SBOM attestation uses `https://spdx.dev/Document/v2.3`. Download the
Debian checksum from the Release, run `sha256sum --check`, and repeat the
container install, remove, state-preservation, and reinstall contract against
the public bytes. Run the public installer in fresh user-owned prefixes on each
native OS and CPU pair, including Alpine and a pre-2.35 glibc image for musl
fallback, then exercise its in-app standalone update route from the preceding
release. Confirm checksum tampering and a failed download preserve the old
binary.

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
