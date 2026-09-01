---
name: release
description: Prepare, verify, publish, or diagnose a Proqi release through the repository's protected tag workflow. Use only when explicitly invoked for this repository.
---

# Proqi Release

This is a repository-local maintainer skill. Act only after explicit invocation.
Never advertise, install, or publish this skill as an end-user Proqi skill.

## Begin safely

1. Read the repository instructions and `docs/RELEASING.md` completely. The
   runbook and checked-in workflows are authoritative over this skill.
2. Verify the branch, `HEAD`, remotes, tags, and complete worktree state. Never
   discard, hide, or publish unexpected changes.
3. Determine whether the user asked to prepare a release, inspect release
   status, diagnose a failure, or proceed toward publication. Preparing files,
   committing them, or pushing `main` does not authorize a tag or release.
4. Read [references/release-notes.md](references/release-notes.md) before
   drafting or reviewing release notes.

## Prepare

1. Derive the change set from the previous stable tag through `HEAD`. Inspect
   commits, user-visible diffs, tests, documentation, and known limitations.
   Do not infer a shipped capability from a commit title alone.
2. Recommend the next semantic version with reasons. Before `1.0`, use a minor
   increase for a new capability or intentional compatibility break, and a
   patch increase for compatible fixes and operational hardening. The user owns
   the final version choice.
3. Update the workspace version in `Cargo.toml`, refresh `Cargo.lock` through
   normal Cargo tooling, create `.github/release-notes/vX.Y.Z.md`, and add or
   update one exact matching entry with three to six concise user-facing items
   in `release-highlights.json`. GitHub notes remain the only changelog. The
   manifest is the bounded in-product projection of those reviewed notes.
4. Run focused checks appropriate to the changed code. Routine release
   preparation then runs the cheap `cargo xtask release-plan vX.Y.Z` contract
   and Git diff hygiene. Run workflow lint only when workflows changed. Full
   check, PTY, coverage, audit, package, crate dry run, full MSRV, rehearsal,
   and Linux parity remain milestone or diagnostic gates as documented in
   `docs/RELEASING.md`; do not repeat them merely because metadata is prepared.
5. Review generated or changed release artifacts explicitly. Never weaken a
   gate, accept snapshots automatically, or claim an unavailable platform check
   passed.
6. Review the exact GitHub notes and matching packaged highlights together.
   Never draft highlights from commit titles alone. Show both texts, the version
   diff, gate results, and any limitations before committing or pushing release
   preparation unless the current instruction already authorizes those exact
   local and remote changes.

## Pre-publication checks

Before asking for publication confirmation:

1. Require the release commit on `main`, its required CI result and immutable
   candidate workflow to be green, and the exact `vX.Y.Z` tag to match the Cargo
   version, notes filename, and reviewed highlights.
2. Confirm that the exact tag is absent locally and remotely and that no
   conflicting GitHub Release or crates.io version exists.
3. Record the full release commit SHA and verify that the worktree contains no
   unreviewed release input.

## Mandatory publication confirmation

Always stop immediately before creating or pushing the release tag. Ask the
user to confirm one exact action in this form:

```text
Create and push the annotated tag vX.Y.Z at <full-commit-sha>, triggering the
public GitHub Release, crates.io publication, and Homebrew tap update?
```

This confirmation is mandatory even if the user previously asked to prepare,
finish, cut, or publish a release, approved the release notes, or approved the
release preparation commit. Do not infer it from earlier authority. Do not
create a local tag, push a tag, call a release workflow, or create a GitHub
Release before the user answers affirmatively to the exact tag and commit.

## Publish after confirmation

1. Revalidate that the commit, CI result, version, notes, and tag state have not
   changed since confirmation. If any changed, stop and ask again with the new
   exact tag and commit. Never move, recreate, or force a public release tag.
2. Create and push one annotated stable tag. Do not create or publish a GitHub
   Release manually. The protected tag is the workflow's sole publication
   trigger.
3. Monitor the tag-triggered `Release` workflow. It selects the exact prior main
   candidate without rebuilding native binaries, publishes crates.io through
   OIDC, publishes and verifies GitHub assets, then notifies the Homebrew tap.
4. If promotion fails, inspect the exact failing step. Rerun the same promotion
   when safe. If the candidate is missing or expired, manually run the
   non-publishing candidate workflow for the exact main SHA or protected tag,
   then retry promotion. Never publish around a failed verification.
5. Verify the public GitHub Release, exact crates.io version, Homebrew formula,
   and installed `proqi --version`. Report each channel independently.

## Recovery boundaries

- The manual `Release candidate` workflow is diagnostic or recovery tooling. It
  never publishes and is not part of an ordinary release.
- If crates.io or any immutable public artifact exists, recover with a new
  semantic version. Never overwrite, delete, yank, or retag merely to make the
  workflow green.
- A failed or ambiguous external operation is not success. Preserve the command
  output and current public state before proposing a retry.
- Do not merge dependency pull requests, alter protection rules, change
  repository settings, or expand distribution channels as a side effect of a
  release.

Finish with the released version, commit and tag, workflow URL and result,
GitHub Release URL, crates.io result, Homebrew result, installed version, and
any skipped or failed verification.
