# Integration test ownership

These rules apply to the complete first-party integration test tree.

## Suite boundaries

- Name each test file for one behavior, contract, or system boundary. The file
  owns only scenarios and helpers for that boundary.
- An integration-test crate root must declare one of two roles in its
  module-level contract. It is either a registry only, or it owns one cohesive
  core contract while registering narrower satellite modules. Top-level tests
  are allowed only in the second form and must belong to that declared core.
- A registry-only suite puts shared harness primitives in a named `support`
  module. Support code may launch fixtures, translate test inputs, poll bounded
  readiness, or decode outputs. It must not own product behavior, scenario
  policy, or assertions that determine whether a feature is correct.
- New and structurally refactored suites import only the support primitives a
  behavior module uses. When touching an older suite that exposes a deliberate
  parent prelude, migrate the affected behavior boundary away from wildcard
  imports instead of extending the implicit surface.

## Refactoring tests

- Split a mixed or oversized test file by behavioral responsibility. Do not
  compact readable tests, rename an unrelated module, or add a source-limit
  exemption merely to satisfy a line ceiling.
- Keep setup and assertions beside the behavior they prove when they are used
  by one module. Promote them to shared support only after a second behavior
  owner needs the same terminal-independent harness operation.
- Give every behavior module a module-level comment that states its boundary.
  Do not create catch-all modules such as `misc`, `common_tests`, or `helpers`.
- Preserve test names and oracle strength during structural moves unless the
  behavior contract itself changes.

## Evidence

- Test one semantic owner at the narrowest layer that can prove it. Add a
  cross-layer integration test only when the contract crosses those layers.
- A regression test must fail for the relevant defect, and its oracle must
  inspect the authoritative state rather than a convenient intermediate
  acknowledgement.
- Keep failure paths in the same behavior module as the successful contract so
  their ownership cannot drift.
