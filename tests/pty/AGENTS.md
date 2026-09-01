# PTY suite ownership

These rules refine `tests/AGENTS.md` for real pseudo-terminal coverage.

- `tests/pty.rs` is the crate registry only. It declares platform-gated
  scenario modules and contains no product scenarios or harness functions.
- `support.rs` owns shared Expect construction, CLI invocation, and bounded
  readiness polling. It contains no product scenario.
- Each other file owns one observable PTY behavior family. State its boundary
  in a module-level comment and keep its fixtures, workflows, and durable
  oracles together.
- Use explicit imports from `support`. Never use `super::*` or reach through
  the crate root for an unrelated scenario helper.
- Keep Expect terminal traffic contained with `log_user 0` unless a test is
  specifically proving bounded output settlement. In that case, explain the
  exception beside the workflow.
- Represent mode transitions explicitly in PTY input. Board shortcuts require
  Board mode, and editor text requires Compose or Edit. Do not rely on timing,
  printable bootstrap behavior, or a previous test's state.
- A real PTY test must assert a durable state, process lifecycle, exact input
  translation, or terminal restoration oracle. Screen matching alone is only
  readiness evidence.
