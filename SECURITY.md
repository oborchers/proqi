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

GitHub private vulnerability reporting is enabled as a repository setting when
the repository becomes public. If that button is unavailable, open a public
Issue containing no vulnerability details and ask the maintainer to enable a
private reporting channel.

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
accidental cross-session mutation, fail open owner forwarding, shell
interpolation, unsafe path traversal, and unverified adjacent-agent submission.
