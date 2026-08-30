# Proqi repository architecture review rubric

Use this rubric for an independent review of the exact recorded revision. Read
the repository instructions and both design-context documents before drawing
conclusions. Inspect implementation, tests, fixtures, history, and the actual
tree; filenames and documentation alone are insufficient evidence.

The review is behavior-preserving. Do not edit files, delete existing
documentation, implement recommendations, remove features, change public or
durable contracts, or write a generated report into the repository.

## Repository and domain structure

- Does the folder and file tree communicate actual responsibilities?
- Should any code move to a different domain, layer, adapter, composition root,
  or test location?
- Are modules split by ownership, or were functions moved into vaguely named
  files merely to satisfy line or complexity limits?
- Does dependency direction remain `domain <- ports <- application <- adapters
  and UI composition` without concrete systems leaking inward?
- Are implementation modules private, public types available from one canonical
  path, and invariants enforced by constructors and appropriately private
  fields?

## Single sources of truth and reuse

- Does each semantic rule, label, compatibility token, geometry calculation,
  state transition, and policy have one named owner in the innermost valid
  layer?
- Are there three or more consumers implementing equivalent behavior that
  should share a dedicated owner? Also flag two consumers when drift would be
  incorrect or unsafe.
- Are shared rendering, terminal-cell width, grapheme truncation, wrapping, hit
  geometry, cursor mapping, selection, folds, and annotation projection derived
  from canonical semantic definitions rather than parallel formatted output?
- Is duplicated code genuinely duplicate semantics, or intentionally separate
  translation at two external boundaries? Recommend consolidation only when
  ownership becomes clearer.
- Are magic strings and numbers internal dispatch keys for a known closed set?
  Prefer typed identifiers, enums, constants, and exhaustive matching while
  retaining compatibility strings at translation boundaries.

## Removal and simplification

- Identify dead, superseded, unreachable, redundant, or speculative code,
  modules, abstractions, dependencies, fixtures, and enforcement.
- Distinguish safe code removal from feature removal. This review does not
  authorize deleting or weakening a product capability.
- Flag abstractions with one accidental caller, optional-field structures that
  combine unrelated concepts, and helpers whose generic names hide domain
  ownership.
- Check whether policy, tests, or documentation enforce the same invariant in
  several brittle ways without improving failure detection.

## Tests and enforcement

- Are behavior-owned tests adjacent to their implementation without obscuring
  production code? Are cross-layer contracts, SQLite/process behavior, CLI,
  and PTY scenarios in appropriate top-level suites?
- Do production and test files stay below the source ceiling through coherent
  responsibility boundaries rather than compression or arbitrary extraction?
- Are tests deterministic through injected clocks, identifiers, paths,
  processes, and external capabilities?
- Is coverage proving meaningful invariants and failure paths, or merely
  restating implementation details, labels, snapshots, and helper structure?
- Are there redundant, over-specific, or overspecified checks that impede sound
  refactors? Never recommend weakening a safety, compatibility, restoration,
  recovery, durability, or accessibility invariant merely to reduce test count.
- Are snapshots reviewed representations of important visible contracts, with
  keyboard/mouse and narrow/wide/shallow/tall behavior covered proportionately?

## Rust design and operational integrity

- Check error types, ownership, borrowing, visibility, constructors, exhaustive
  enums, fallible boundaries, cancellation, bounded teardown, thread/process
  guards, filesystem safety, and absence of unsafe or production panics.
- Check SQLite ownership, durable migrations, history/undo, idempotency,
  operation sequencing, recovery, and content-redacted journals for duplicated
  or misplaced policy.
- Check whether terminal, filesystem, time, environment, clipboard, process,
  network, and identifier behavior is injected at the correct boundary.
- Review dependencies and features for unused weight, adapter leakage, or
  parallel implementations of capabilities already present.
- Inspect public CLI/JSON, prefixed identifiers, configuration, protocol
  vocabulary, snapshots, and packaging fixtures for accidental drift.

## Historical and linter-integrity review

- Use relevant history or blame to understand suspicious extractions, recent
  integration seams, and why duplicate-looking paths exist.
- Explicitly identify attempts to game maximum file size, function length,
  complexity, nesting, or architecture checks by compacting readable code,
  hiding related tests, or moving unrelated functions into catch-all modules.
- Examine the interaction between inline tests and file limits. Recommend an
  adjacent `tests.rs` or `tests/` module when tests obscure the production
  responsibility; do not move tests solely to lower a line count.
- Evaluate whether existing `AGENTS.md` rules capture stable ownership patterns
  and whether a nested instruction file is warranted. Avoid policy for one-off
  implementation details.

## Required report

Return:

1. An executive verdict on overall architectural health.
2. Findings ordered by impact. For each finding provide exact file/symbol
   evidence, the violated or unclear responsibility, concrete risk, smallest
   behavior-preserving correction, compatibility implications, and tests that
   should move or prove the change.
3. Explicit deduplication, relocation, removal, and test-restructuring
   candidates, including a statement when a category has no justified change.
4. Keep-as-is decisions for structures that looked suspicious but are correctly
   separated.
5. A dependency-ordered refactor sequence distinguishing prerequisites from
   independent work.
6. Stable architecture rules that should be added to root or nested
   `AGENTS.md`, if any.
7. Uncertainties or product decisions that cannot be resolved from repository
   evidence.

Do not inflate the report with naming preferences or generic Rust advice. A
clean review may conclude that no refactor is justified, but it must cite the
evidence inspected to support that conclusion.
