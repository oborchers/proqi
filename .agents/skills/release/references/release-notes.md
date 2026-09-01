# Proqi release notes

Release notes explain what changed for a person who uses Proqi. They are not a
commit log, test report, or architecture summary.

## Evidence

Build the draft from evidence between the previous stable tag and the proposed
release commit:

- User-visible behavior and CLI contracts.
- Fixed regressions and meaningful reliability improvements.
- Installation, compatibility, migration, privacy, or recovery changes.
- Tests and documentation that substantiate each claim.
- Known limitations that materially affect adoption or upgrading.

Verify every command, shortcut, platform, and distribution claim against the
current repository. Never turn an internal refactor into a user benefit without
evidence, and never claim that a release is flawless, fully compatible, or
universally supported.

## Version choice

Proqi follows semantic versions while it is pre-`1.0`:

- Patch: compatible bug fixes, security corrections, performance work, and
  operational hardening without a new user capability.
- Minor: a new user-visible capability or an intentional breaking change to a
  pre-`1.0` CLI, configuration, or machine-readable contract.

When both apply, use the higher category. Explain the recommendation and let
the user approve the exact version.

## Structure

Use this compact shape and omit optional sections that add no useful
information:

```markdown
# Proqi X.Y.Z

One sentence describing the release's concrete outcome.

## Highlights

- User-visible outcome with the relevant interaction or command.
- Fixed failure and what is now preserved or recovered.
- Distribution or compatibility change, when relevant.

## Breaking changes

Required migration or changed behavior, with a safe action for the user.

## Install

Exact commands only when installation or upgrade guidance changed or is
important for this release.

## Known limitations

Material limitations that remain after this release.
```

The title must match the Cargo version and filename exactly:
`.github/release-notes/vX.Y.Z.md` contains `# Proqi X.Y.Z`.

## Packaged highlights

Every represented GitHub note has one exact matching version in
`release-highlights.json`. Add three to six short user-facing outcomes for the
new version. They are displayed inside the installed product, so each item must
stand on its own without links, installation commands, internal architecture,
or a surrounding release-note paragraph.

Draft the GitHub note and packaged items from the same inspected user-visible
diffs, tests, documentation, and known limitations. Commit titles alone are
never evidence for either artifact. Review the exact two texts together before
release preparation is committed. The release and package gates require all
note versions, manifest versions, the Cargo version, and the requested tag to
agree exactly.

## Writing rules

- Lead with outcomes, then explain the interaction that changed.
- Prefer five to eight high-signal highlights over exhaustive coverage.
- Put breaking changes and required user action before ordinary highlights.
- Use the product's real labels, shortcuts, commands, and identifier spelling.
- Distinguish shipped behavior from experimental, deferred, or unsupported
  behavior.
- Mention internal implementation only when it explains reliability, privacy,
  compatibility, or recovery that users can observe.
- Keep installation commands copyable and channel-specific. Do not advertise a
  channel until its public artifact has a verified release contract.
- Link an issue only when it adds necessary context. The note must still make
  sense without following the link.
- Use plain international English, proper Unicode, and no emoji. Do not use em
  or en dashes as sentence separators.
- Do not add thanks, marketing superlatives, contributor claims, or security
  details without verified evidence and approval.

## Review checklist

Before approving the note, confirm:

1. Every highlight is present in the tagged source and covered by evidence.
2. The note states any pre-`1.0` breaking behavior and necessary migration.
3. Commands, paths, names, shortcuts, minimum versions, and platforms are exact.
4. Privacy, durability, and security claims are no broader than their tests and
   architecture contracts.
5. The note contains no temporary paths, private identifiers, credentials,
   unpublished URLs, or internal review artifacts.
6. The filename, title, Cargo version, and proposed tag all agree.
7. The matching manifest entry contains three to six reviewed outcomes and no
   claim broader than the GitHub note evidence.
