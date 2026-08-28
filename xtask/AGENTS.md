# Proqi engineering-operations contract

The repository root contract applies here. `xtask` is the sole project-owned
command surface for development, CI, packaging, and release verification.

## Command ownership

- Keep subcommands thin, deterministic orchestrators around standard tools and
  repository policy. Do not add a shell script, Make target, or workflow-only
  implementation when `xtask` can own the operation portably.
- `quality` is deliberately test-free. `test` owns the deterministic suite and
  `check` is exactly the canonical aggregate of `quality` followed by `test`.
  CI jobs may split these phases but must not redefine or redundantly rerun the
  complete suite.
- A policy check reports actionable paths and reasons, has accepted and rejected
  fixture coverage, and scans the repository independently of developer machine
  state. Never weaken a policy to accommodate one implementation.
- Every `AGENTS.md` at any depth has a sibling relative
  `CLAUDE.md -> AGENTS.md` symlink. Instruction validation remains part of the
  canonical quality path.
- Keep build, package, coverage, and review artifacts below ignored `target/` or
  a securely created temporary directory. A verification command never
  publishes, installs globally, edits Git configuration, or changes repository
  settings without a separately authorized command.

## Compatibility and verification

- Preserve the pinned toolchain, MSRV, platform package contracts, source-only
  crate allowlist, Debian install/remove/reinstall contract, and exact artifact
  identity across release stages.
- Treat GitHub Actions as scheduling and permissions configuration. Commands,
  policy, and package behavior stay locally executable through `cargo xtask`.
- Tests for operations tooling must not require credentials, user configuration,
  the network, Docker, or a particular host path unless that capability is the
  explicit subject of a separately gated command.
