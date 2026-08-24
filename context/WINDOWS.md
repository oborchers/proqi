# Future Windows implementation brief

Status: preparation only. Windows is not a Proqi `v0.1.0` support or release
target.

This document defines the security and product questions a later Windows goal
must resolve. It does not authorize a named-pipe transport, Windows updater,
release archive, signing configuration, PTY harness, or support claim.

## Current behavior

| Capability | Current Windows state |
| --- | --- |
| Compilation and deterministic tests | Configured in the ordinary GitHub Actions test matrix. A local macOS run cannot prove the Windows job result. |
| Scriptable CLI and SQLite | Terminal-independent code remains part of the Windows test signal. |
| Interactive TUI | Not a public support claim and not covered by Windows PTY automation. |
| Owner-control endpoint | Not prepared or advertised. Runtime metadata contains no control protocol or endpoint. |
| Active-session forwarding | Unsupported. The transport refuses both server creation and client connection. |
| Capability discovery | JSON reports `active_session_control: false`; human output reports that active control is unavailable. |
| Active-session mutation | Fails closed as `session_busy`. It never writes around the active owner. |
| Packaging and updates | No Windows artifact, installer, updater, signing, or same-pane restart contract. |

Ordinary Windows compile and terminal-independent tests remain useful regression
signals. The `v0.1.0` package and release gates cover only the three documented
macOS and Linux release targets.

## Required security boundary

### Pipe namespace and ownership

A future transport must use a local named-pipe namespace with an unpredictable,
installation-scoped instance component. The exact namespace and name format are
unresolved. Candidate forms must be reviewed against supported Windows versions
before one becomes a durable protocol.

The server must create the first pipe instance with all of these properties:

1. An explicit security descriptor. The Windows default is unacceptable because
   it grants read access to Everyone and anonymous users.
2. A DACL that grants only the intended current user or logon SID the minimum
   required rights. The later goal must decide whether Proqi coordinates across
   logon sessions or stays within one logon session.
3. Remote clients rejected with `PIPE_REJECT_REMOTE_CLIENTS`.
4. Non-inheritable handles unless a narrowly reviewed child-process contract
   requires inheritance.
5. First-instance protection so an untrusted process cannot pre-create the name
   and impersonate the owner.
6. Bounded instance count, input buffer, output buffer, connection wait, and I/O
   deadlines.

Runtime metadata remains descriptive. A pipe name, PID, instance ID, or metadata
file never proves ownership by itself.

### Client identity at the server

Every accepted connection must be authenticated before a request reaches the
owner reducer:

1. Read a request only within the existing message bound.
2. Call `ImpersonateNamedPipeClient` and fail closed if impersonation fails.
3. Open the impersonated thread token with query-only access.
4. Read `TokenUser` through `GetTokenInformation`.
5. Validate and compare the client SID with the expected owner or logon SID.
6. Call `RevertToSelf` on every path. Microsoft states that a process must stop
   if reversion fails because it would otherwise continue under the client
   context.
7. Reject anonymous, cross-user, remote, malformed, or unverifiable clients.

The peer PID may be recorded as supplementary evidence. It must never be the
authentication decision. The current `interprocess` documentation explicitly
warns that PID-based security lookups can race with process exit, handle
inheritance, and PID reuse.

### Server identity at the client

The client also needs proof that it connected to the advertised owner. A later
design must combine pipe ACL and owner inspection with a stable process identity.
The process identity should include the PID and creation time obtained from an
open process handle. PID alone is insufficient because Windows reuses process
identifiers.

The client must verify the server token SID and the process identity advertised
in private runtime metadata before sending content. If any check is unavailable,
the command returns `session_busy` and preserves the thought.

### Stale metadata and process lifetime

The authoritative session lease stays separate from endpoint metadata. A later
Windows coordinator must:

1. Validate the lease before trusting metadata.
2. Compare PID plus process creation time to prevent PID-reuse confusion.
3. Treat an absent process, closed pipe, mismatched token, or mismatched creation
   time as stale.
4. Remove stale metadata only after proving that no verified owner holds the
   lease.
5. Never delete or replace an endpoint merely because connection failed once.
6. Prevent pipe handles from leaking into child processes.

## Protocol invariants to preserve

Windows must use the same typed owner-control protocol as Unix after transport
authentication:

- Maximum encoded message size remains 1,048,576 bytes.
- Requests carry typed prefixed identifiers and a supported protocol version.
- Request IDs are replay keys. Reuse with a different payload is rejected.
- Durable operation IDs preserve mutation idempotency across retries.
- Responses must match the request ID and negotiated protocol.
- Connection, I/O, reducer response, and shutdown waits remain bounded.
- Queues and accepted-receipt caches remain bounded.
- Malformed, oversized, timed-out, ambiguous, or unverifiable traffic fails
  without a direct database fallback.

The future implementation must decide whether Windows overlapped I/O integrates
with the existing worker lane or warrants a small platform adapter. It must not
introduce a second application protocol.

## Safe Rust options

No dependency is selected by this preparation milestone.

1. The current `interprocess` local-socket abstraction maps Windows local sockets
   to named pipes, but its public `ListenerOptions` does not expose the explicit
   Windows security descriptor required by Proqi. Its Windows peer credentials
   expose a PID, which is not sufficient authentication.
2. Tokio exposes bounded named-pipe options such as remote-client rejection and
   first-instance creation. Passing a custom `SECURITY_ATTRIBUTES` value uses an
   unsafe raw-pointer API and would also add a runtime that Proqi does not
   otherwise need.
3. The `windows` and `windows-sys` crates expose the required Win32 APIs, but raw
   FFI calls require unsafe code and careful handle, token, SID, and impersonation
   lifetime management.
4. A later goal should first evaluate a maintained safe wrapper that supports an
   explicit DACL, token impersonation, peer SID verification, overlapped I/O, and
   non-inheritable handles. If none satisfies the contract, a narrowly isolated
   Windows adapter requires an architecture decision, a documented safety
   argument, and focused adversarial tests before the repository unsafe rule can
   change.

First-party unsafe Rust remains forbidden for `v0.1.0`.

## Required Windows tests

A later implementation is incomplete until native Windows CI proves:

1. Same-user and intended same-logon-session clients are accepted.
2. A different user, anonymous client, remote client, and unintended logon
   session are denied.
3. A malicious process cannot pre-create or replace the owner pipe.
4. Client and server token SIDs are verified in both directions.
5. PID reuse, stale creation time, inherited handles, and stale metadata are
   rejected.
6. Oversized, malformed, replayed, wrong-version, and wrong-prefix messages fail
   closed.
7. Timeout, owner crash, client crash, full lane, and shutdown paths terminate
   without deadlock or durable ambiguity.
8. Accepted operation retries are idempotent and conflicting identity reuse is
   rejected.
9. Active-session mutation never writes directly around an owner.
10. Cross-process tests run as both the current user and a separate restricted
    user. Unit fakes alone are insufficient for the security claim.

The current Windows-only regression test intentionally proves only that both
server and client transport paths return `Unsupported`. It is not a named-pipe
security test.

## Windows Terminal and ConPTY

ConPTY is the later automation boundary for real terminal behavior. Microsoft
specifies UTF-8 on the pseudoconsole channel and recommends servicing input and
output on separate threads to avoid filled-buffer deadlocks. A native harness
must cover startup, Unicode input, mouse input, resize, bracketed paste where
supported, signals or console control events, terminal restoration, and clean
shutdown in Windows Terminal and the CI host.

Closing a pseudoconsole can emit a final frame and can deadlock if output is not
drained. Handle lifetime and teardown therefore require explicit tests. No
Windows PTY result participates in the `v0.1.0` release gate.

## Packaging, signing, updates, and restart

A later Windows goal must separately decide:

- Archive, MSIX, WinGet, or another installation channel.
- Authenticode signing identity, timestamping, and trust policy.
- Upgrade ownership and rollback behavior.
- Console control event and terminal restoration behavior during replacement.
- Whether a launcher, supervisor, or explicit next-start resume replaces the
  Unix same-process `exec` contract.

Windows has no direct equivalent of Unix `exec` that replaces the current
process image while preserving the same PID and descriptors. Same-pane restart
therefore needs a separately tested launcher or terminal handoff design. Proqi
must not imply that the macOS and Linux Homebrew restart contract already works
on Windows.

## Decisions reserved for the future goal

1. User SID versus logon SID scope.
2. Pipe namespace and stable naming format.
3. Safe wrapper versus a reviewed isolated unsafe adapter.
4. Synchronous worker integration versus overlapped I/O.
5. Minimum Windows and Windows Terminal versions.
6. ConPTY harness and CI account topology for cross-user tests.
7. Packaging, signing, installation, update, and restart ownership.
8. Public support policy and release artifact targets.

## Primary references reviewed

- [Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- [CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)
- [ImpersonateNamedPipeClient](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient)
- [OpenThreadToken](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openthreadtoken)
- [GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
- [EqualSid](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-equalsid)
- [RevertToSelf](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-reverttoself)
- [GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)
- [GetNamedPipeServerProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid)
- [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes)
- [Pseudoconsoles](https://learn.microsoft.com/en-us/windows/console/pseudoconsoles)
- [Creating a Pseudoconsole Session](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [`interprocess::local_socket::PeerCreds`](https://docs.rs/interprocess/latest/interprocess/local_socket/struct.PeerCreds.html)
- [`interprocess::local_socket::ListenerOptions`](https://docs.rs/interprocess/latest/interprocess/local_socket/struct.ListenerOptions.html)
- [`tokio::net::windows::named_pipe::ServerOptions`](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html)
