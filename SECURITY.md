# Security Policy

## Supported versions

Proqi supports only the latest stable release. Older releases and unreleased
development snapshots do not receive separate security fixes. Upgrade to the
latest stable version before reporting a problem when it is safe to do so.

## Report a vulnerability privately

Do not disclose a suspected vulnerability in a public Issue, pull request,
Discussion, or other public channel.

Use GitHub's private vulnerability reporting for this repository:

<https://github.com/oborchers/proqi/security/advisories/new>

Select **Report a vulnerability** and include:

- The affected Proqi version and operating system.
- The expected security boundary and observed behavior.
- The smallest safe reproduction steps.
- The potential impact.
- Any suggested mitigation, if available.

Remove prompt content, clipboard data, credentials, personal paths, and other
private material unless it is essential to reproduce the vulnerability. If
sensitive material is essential, explain that first and wait for guidance
before attaching it.

The release checklist requires GitHub private vulnerability reporting to be
enabled before `v0.1.0` is published. If that button is unavailable, open a
public Issue containing no vulnerability details and ask the maintainer to
enable a private reporting channel.

This project does not publish response-time or bounty commitments. Reports are
reviewed and coordinated through the private GitHub advisory before any public
disclosure.

## Security scope

Security-sensitive behavior includes local session isolation, typed control
messages, filesystem permissions, SQLite migration and recovery, clipboard and
attachment handling, subprocess construction, update checks, release artifacts,
and Homebrew coordination.

Proqi does not claim a security boundary against an administrator or a malicious
process already running as the same operating-system user. It does prevent
accidental cross-session mutation, fail-closed owner forwarding, shell
interpolation, unsafe path traversal, and unverified adjacent-agent submission.

## Local diagnostics

Proqi writes typed JSONL lifecycle, command, and submission-state events to its
platform-native data directory. These events omit thought content, clipboard
content, session names, workspace paths, pane identifiers, and raw external
responses. Submission records contain typed Proqi identifiers, direction,
provider kind, state, and stable outcome codes only.

Each running instance keeps at most five 1 MiB segments. Inactive logs are
pruned toward a 20 MiB installation-wide ceiling. A live process owns a file
lock, so retention never deletes an active instance's logs. Files and generated
support bundles use user-only permissions on supported systems.

`proqi diagnostics collect --output PATH` creates a versioned, local JSON
bundle. It does not upload the result and refuses to overwrite an existing
file. Review every bundle before sharing it because stable identifiers and
operational timestamps can still provide diagnostic context.
