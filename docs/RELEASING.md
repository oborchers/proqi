# Releasing Proqi

This runbook describes the release boundary. It does not grant publication
authority by itself.

## Release invariants

- `Cargo.toml` is the only version source.
- Stable tags use exact `vX.Y.Z` syntax and must equal the Cargo version.
- Release artifacts exist only for Apple silicon macOS, Intel macOS, and
  x86-64 GNU Linux.
- Every archive includes the executable, MIT license, third-party notices,
  standalone installation marker, and Bash, Zsh, and Fish completions.
- Every archive has a SHA-256 file, SPDX 2.3 JSON SBOM, provenance attestation,
  and SBOM attestation.
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
cargo +1.88.0 xtask msrv
cargo xtask release-plan v0.1.1
cargo xtask release-rehearsal
actionlint .github/workflows/ci.yml .github/workflows/release-candidate.yml .github/workflows/release.yml
zizmor --pedantic .github/workflows/ci.yml .github/workflows/release-candidate.yml .github/workflows/release.yml
git diff --check
```

Inspect the rehearsal archive members, notices, checksum, SBOM, formula, social
preview, diagram, and README GIF. Rehearsal output remains ignored below
`target/release-rehearsal`.

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
   candidate for seven days.
5. Inspect all three archives, checksums, SBOMs, attestations, formula, manifest,
   source commit, and artifact digest from that successful run.
6. Create and push the annotated stable tag at the exact candidate commit. The
   tag-triggered `Release` workflow downloads and verifies the candidate, adds
   tag-bound attestations, and publishes those same bytes without rebuilding.
7. If the candidate is absent or expired, delete the unpublished tag, dispatch
   a replacement candidate for the same tag and commit, then recreate the tag.
8. Verify the published Release and every downloaded asset. Publication sends
   a scoped `proqi_release_published` event to the public tap, which verifies
   and publishes the formula. Its scheduled sync remains a recovery path.

The release workflow never cancels an in-progress tag release. Any failed
target, smoke test, checksum, SBOM, attestation, or formula generation blocks
draft creation.

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
release before committing one exact formula update. The scheduled and manually
dispatchable paths remain available for reconciliation. No personal access
token is stored in either repository.

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

Repeat for Intel macOS and x86-64 GNU Linux. Verify that the SBOM attestation
uses `https://spdx.dev/Document/v2.3`. Install through the public tap, run
`brew test proqi`, launch the TUI, create and resume one session, and verify an
explicit `proqi update check --json` sees `0.1.1`.

Ordinary uninstall must leave user data intact:

```shell
brew uninstall proqi
```
