# Open Source Contributor Governance Spike

Status: research report, no policy or setting changes implemented

Evidence cutoff: 2026-09-09

Repository baseline: `8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea`
Audience: Proqi maintainers first, prospective contributors second

## Executive verdict

Proqi does not need a governance program. It needs a small, explicit contributor
contract backed by the controls it already mostly has.

The repository is substantially better prepared for public contribution than a
typical young, single-maintainer project. It already has an MIT license, a
detailed contribution guide, a pull request template, Contributor Covenant 2.1,
a security policy with private vulnerability reporting, structured issue forms,
an all-path `CODEOWNERS` entry, an MSRV, deterministic test and snapshot rules,
cross-platform CI, a stable aggregate required check, strict current-`main`
freshness, protected release tags, least-privilege default Actions permissions,
full-SHA action references, dependency policy checks, release SBOMs, and build
attestations.[^1][^2][^3][^4][^5][^6][^7][^8][^9]

The smallest professional baseline to adopt now is therefore incremental:

1. Keep the current strict required-status rule, the owner-controlled release
   boundary, private vulnerability reporting, secret scanning and push
   protection, and bounded Dependabot patch and minor auto-merge.
2. Add an original, short AI-assisted contribution clause. Permit assistance,
   require an accountable human who understands and can revise the change,
   require disclosure of material use and its scope, and prohibit unreviewed
   prompt transcripts, session links, secrets, or private user data.
3. Tighten the existing pull request template around issue linkage, risk areas,
   exact verification actually run, skipped platform checks, migration and
   snapshot review, and material AI assistance.
4. Turn the repository-level full-SHA Action policy on. Replace “all actions”
   with the exact external action repositories currently used, plus the CodeQL
   action only if that pilot proceeds. Keep default `GITHUB_TOKEN` permissions
   read-only and workflow-level elevation narrow.
5. Change fork workflow approval from first-time contributors to all external
   contributors, at least while one maintainer personally inspects untrusted
   Rust build scripts, tests, workflow changes, and platform behavior.
6. Pilot CodeQL default setup for Rust and GitHub Actions without making it a
   required check until its signal and cost have been observed. Preserve the
   enabled immutable-release setting and verify Proqi's draft-first promotion
   flow against it before the next publication.
7. Give conduct reports a real private route distinct from the vulnerability
   channel. Preserve the current security route for security reports only.

Practices to add only after contributor or maintainer volume increases include a
formal maintainer roster, granular triage roles through an organization, enforced
code-owner review, non-author approvals, stale-approval dismissal, and a documented
maintainer promotion process. A merge queue is unavailable to this personal-account
repository and would be needless at current volume.[^26][^35]

Proqi should avoid a CLA, DCO sign-off, mandatory signed commits, multiple
approvals, an elaborate governance constitution, blanket issue-first rules for
all changes, a blanket AI ban, and mandatory prompt transcript publication.
Those measures would add identity, tooling, legal, or review friction without
addressing the present failure modes. The strongest current controls are bounded
scope, human accountability, risk-specific evidence, owner review, strict CI,
and a separately authorized release path.

## Method and evidence boundary

This spike separates four kinds of evidence:

- **Checked-in policy** is what contributors can read and propose changes to.
- **Checked-in automation** is what workflows and repository-owned tools do.
- **GitHub enforcement** is what the platform blocks or permits independently of
  prose. Owner-authenticated, read-only REST and GraphQL queries were used on
  2026-09-09 for settings that are not all visible anonymously.[^10][^11][^12][^13][^14]
- **Observed workflow** is how recent public issues and pull requests actually
  progressed. This evidence is used to identify process friction, never to rate
  people.

The local baseline was fixed at the ticket's exact commit. Remote `main` was also
checked because pull request 88 landed after that baseline and established the
current bounded Dependabot policy.[^19] Peer documents were read at pinned commits,
not inferred from search snippets or default-branch URLs. Absence means that the
named file or setting was not present in the inspected public tree. It does not
mean a project lacks private practice.

This is not a legal opinion, a security audit, or evidence that any private
setting exists. Recommendations that change policy or GitHub configuration need
owner approval and separate implementation work.

## 1. Current-state inventory

Classification meanings:

- **Present and adequate** means proportionate for Proqi now.
- **Present but weak** means useful, but policy and enforcement can drift or the
  current wording leaves repeatable friction.
- **Absent and important now** means the present contribution or threat evidence
  justifies a small addition.
- **Intentionally unnecessary** means adding it now would create more ceremony
  than protection.

| Surface | Evidence at cutoff | Classification | Finding |
|---|---|---|---|
| Public identity and license | Public repository, MIT detected; `LICENSE` and `Cargo.toml` agree | Present and adequate | Clear outward license and package metadata. |
| README contributor entry point | README links contribution, security, platform, privacy, compatibility, release, and diagnostics material | Present and adequate | Contributors can reach the rules from the project front door. |
| Contribution guide | Setup, architecture, issue-first boundaries, focused and canonical gates, snapshots, source limits, PR evidence, and inbound MIT terms are explicit | Present and adequate | This is the canonical contributor contract and should remain the single owner of most new guidance.[^2] |
| Pull request template | Summary, MIT notice, security redirect, canonical gate, focused tests, docs, secret, and snapshot checklist | Present but weak | Add issue or decision linkage, risk-specific evidence, skipped checks, migrations and contracts, platform evidence, and material AI use.[^3] |
| Code of Conduct | Contributor Covenant 2.1 is adapted and names the repository owner as enforcer | Present but weak | “Report privately” is not paired with a dependable private conduct channel. A content-free public issue is a poor fallback for sensitive conduct reports.[^4][^40] |
| Security policy | Latest stable only, private reporting, safe reproduction, redaction, scope, no SLA or bounty promise, and diagnostics handling | Present and adequate | It is unusually specific and aligns with enabled private vulnerability reporting.[^5][^31] |
| Support policy | Latest `0.x`, supported platforms, issue routes, and security boundaries appear in README and SECURITY | Intentionally unnecessary | A separate `SUPPORT.md` would duplicate current ownership. Revisit when questions or unsupported-version requests become material. |
| Issue intake | Bug and feature forms, blank issues disabled, security and contribution links | Present and adequate | The forms protect privacy and route security reports before free-form input.[^6] |
| PR and issue labels | Existing labels cover bugs, docs, dependencies, accessibility, platform-relevant Rust work, good first issues, help, questions, and dispositions | Present and adequate | Do not add a taxonomy until recurring triage needs prove it. |
| Maintainer or governance file | No dedicated `MAINTAINERS.md` or `GOVERNANCE.md`; public repository ownership and `CODEOWNERS` identify the current owner | Intentionally unnecessary | A constitution adds no decision capacity while there is one primary maintainer. A short maintainer and trust policy becomes useful before granting another person write access. |
| `CODEOWNERS` | `* @oborchers`; GitHub code-owner review is not required | Present and adequate | It is a routing and ownership signal, not an approval control. Requiring the sole owner to approve every PR would not protect owner-authored changes.[^25] |
| Inbound licensing | Contribution guide says MIT inbound equals outbound and explicitly declines CLA and DCO | Present and adequate | GitHub's Terms also apply repository-license terms to contributions. Preserve this simple model unless counsel identifies a concrete ownership or relicensing need.[^2][^36] |
| AI-assisted contribution policy | No checked-in policy | Absent and important now | Recent external pull requests already disclose AI assistance in different forms. A small rule can establish responsibility, privacy, and evidence expectations without banning tools. |
| Architecture contract | Layer direction, canonical ownership, deterministic ports, terminal boundaries, typed identifiers, and source limits are documented | Present and adequate | Contributors are told when to read product and architecture context. |
| MSRV and platform policy | Rust 1.88, macOS and x86-64 GNU/Linux support, locked dependencies, MSRV and platform CI | Present and adequate | Cargo's `rust-version` field lets the resolver and tooling reason about compatibility; committing `Cargo.lock` keeps application builds reproducible; Proqi tests that declared floor.[^7][^37][^38] |
| Tests and failure paths | Focused tests while developing, `cargo xtask check` before submission, regression tests, deterministic seams, PTY and platform expectations | Present and adequate | Evidence is risk-sensitive rather than “CI is green.” |
| Snapshots and generated artifacts | Visible changes require reviewed Insta snapshots; auto-accept is forbidden; pending snapshots fail | Present and adequate | The human review invariant is explicit and should also appear in the PR template. |
| SQLite migrations | Architecture owns append-only migrations and recovery invariants, but contributor-facing PR evidence is less explicit | Present but weak | Add a short requirement for forward upgrade, fixture, rollback or recovery reasoning, and compatibility evidence when migrations change. |
| CLI and JSON contracts | Pre-1.0 breaking changes must update current-contract fixtures and prepared release notes | Present and adequate | Add a matching PR-template prompt so reviewers see the compatibility decision. |
| Branch rules | Active `main` ruleset blocks deletion and non-fast-forward updates, requires a PR, permits squash or rebase, and requires `Required CI result` against current `main`; owner has always-bypass | Present and adequate | Preserve strict freshness. The bypass is necessary for single-owner recovery but remains a high-value account-compromise path.[^11][^20][^21] |
| Review enforcement | Zero approving reviews, no stale dismissal, no last-push approval, no required code-owner review, no required conversation resolution | Present but weak | Zero approvals is proportionate while the sole owner is the only merger. Require conversation resolution now. Add review-count controls only after a second trusted maintainer exists. |
| Merge methods | Repository globally permits merge, squash, and rebase; the `main` ruleset narrows PR merges to squash or rebase | Present and adequate | The ruleset is enforcement. Contributor guidance should state that maintainers own final history shaping. |
| Strict freshness and branch updates | Strict status freshness is on; repository `allow_update_branch` is off | Present but weak | Recent green external PRs became behind `main`. Enable the update suggestion after deciding who may update contributor branches and documenting `maintainerCanModify`. Do not loosen freshness.[^17][^18][^21] |
| Default Actions permissions | Default token is read-only; Actions cannot approve PR reviews; ordinary CI declares `contents: read` | Present and adequate | Continue job-local elevation only where publication or repository mutation requires it.[^8][^12][^23] |
| Action source policy | Every checked-in external action is full-SHA pinned, but GitHub allows all actions and does not enforce SHA pinning | Present but weak | Turn on platform enforcement and allow only the exact action repositories currently required. This prevents a future workflow change from silently weakening the checked-in convention.[^8][^12][^22] |
| Dangerous workflow triggers | No `pull_request_target` or privileged `workflow_run` path found | Present and adequate | Keep it that way unless a reviewed privilege-separation design is necessary. Never build or execute fork content in privileged context.[^23] |
| Fork workflow approval | Approval policy is `first_time_contributors` | Present but weak | A merged innocuous change graduates an external actor for later Actions runs. Use all-external approval while review volume is low and CI executes untrusted Rust code.[^12][^22] |
| CI aggregation | One stable `Required CI result` owns required status; docs-only classification avoids unnecessary product work | Present and adequate | This avoids ruleset drift as the matrix changes. |
| Dependency updates | Weekly grouped Cargo, Actions, and toolchain updates; security group; `cargo-deny`, `cargo-audit`, source and license policy; bounded patch and minor auto-merge on current `main` | Present and adequate | Preserve the new policy. Majors and changes that fail any strict check remain human decisions.[^8][^19][^27] |
| Dependency review | Full-graph security and license checks exist; GitHub dependency-review enforcement is not present | Present but weak | Add the dependency review action as a narrowly scoped change detector after confirming it adds signal beyond existing Cargo gates.[^28] |
| Secret scanning | Secret scanning and push protection enabled; non-provider patterns and validity checks disabled | Present and adequate | Preserve. The disabled extensions are not a present blocker. Push protection has documented size and pattern limits, so policy must still prohibit secrets.[^10][^30] |
| Code scanning | CodeQL default setup reports `not-configured` for detected Rust and Actions | Absent and important now | Pilot default setup. Rust's buildless mode makes the experiment low-maintenance, but default setup excludes fork PRs, so it is defense in depth, not a fork gate.[^14][^29] |
| Private vulnerability reporting | Enabled; issue form and SECURITY link to it | Present and adequate | Preserve as the single security intake route and keep conduct reporting separate.[^10][^31] |
| Release environment | `release` exists, accepts only `v*.*.*`, has no required reviewer, and currently permits admin bypass | Present and adequate | A protected tag is the owner's explicit publication act. Secrets and write scopes are confined to the environment job.[^9][^13][^32] |
| Release provenance | Candidate promotion verifies exact commit, artifact digest, every byte, SBOM, and attestations before publication | Present and adequate | Stronger than most peers. Attestations prove provenance, not that content is safe.[^8][^34] |
| Stable tags | Active `v*` ruleset restricts creation, deletion, and non-fast-forward updates to the owner bypass actor | Present and adequate | Preserve. |
| Immutable releases | The owner-authenticated REST endpoint reports `enabled: true` and `enforced_by_owner: false` | Present and adequate | Preserve the enabled setting. Verify the draft-first promotion, reconciliation, and recovery flow against immutability before the next publication rather than reopening the enablement decision.[^33][^64] |
| Auto-merge | Repository allows auto-merge; current `main` requests squash auto-merge only for genuine Dependabot patch and minor updates | Present and adequate | Preserve this bounded case. Do not offer general contributor auto-merge yet.[^19] |
| Release and package authority | Tag push triggers promotion, crates.io uses short-lived trusted publishing, GitHub release and tap notification are restricted to release workflow | Present and adequate | Outside contributors and ordinary CI receive no publication authority. crates.io trusted publishing replaces a long-lived secret with a short-lived token bound to the configured GitHub repository, workflow, and environment.[^8][^9][^62] |
| Contributor recognition | Git history, PR authorship, release notes, and advisory credit are available; no formal rollup | Intentionally unnecessary | Add a recurring acknowledgment only if contributor volume makes omission visible. Do not create a ceremony to recognize one or two PRs. |

### What is checked in versus enforced

Several apparent controls are policy only:

- Reading architecture context, running the canonical local gate, reviewing
  snapshots, reporting skipped checks, respecting source limits, and declaring
  inbound MIT terms depend on contributor and reviewer behavior.
- `CODEOWNERS` requests or signals ownership, but does not require approval in
  the current ruleset.
- Full-SHA pinning is checked into every current workflow, but the repository
  setting still allows an unpinned action to be introduced.
- The release runbook describes the intended boundary. The tag ruleset,
  environment branch policy, job permissions, OIDC claims, and trusted publisher
  configuration are the enforcement layers.

Conversely, strict current-base status, deletion and force-push protection,
fork workflow approval, default token permissions, private vulnerability
reporting, secret scanning, and push protection are platform controls. They do
not need duplicate prose everywhere, but contributors need a concise explanation
of the behavior they will encounter.

## 2. Evidence from recent public contribution flow

The useful unit of analysis is the interaction between scope, evidence, review,
and platform controls.

### A large unscoped architecture change

Pull request 66 proposed a second agent-submission backend. It changed 59 files
with 3,002 additions, touched durable architecture, and arrived from a fork with
maintainer modification disabled. It was closed unmerged and was conflicting at
the final observed state.[^15] The PR itself acknowledged that the contribution
guide called for prior discussion but proceeded directly.

The process lesson is not that large external work is unwelcome. It is that
architecture, durable contracts, new backends, and broad refactors have a high
coordination cost before implementation quality can even be assessed. The
existing issue-first rule was correct. A PR template should ask for the accepted
issue or design decision, and maintainers should be able to close or redirect
work early when that link is absent. An arbitrary hard line-count rejection
would be inferior because some migrations are inherently broad. Scope agreement
and coherent slices are the meaningful tests.

### A bounded, issue-led compatibility change

Pull request 80 followed issue 74, changed 11 files with 162 additions, included
protocol and live qualification evidence, passed the full macOS and Linux matrix,
merged current `main`, and was merged.[^16] The author disclosed AI assistance.
This is evidence that Proqi's desired path works: establish the problem, keep the
change bounded, prove the external integration against reality, satisfy the
canonical gate, and restore freshness before merge.

### Green CI did not prove a terminal lifecycle change safe

Pull request 81 added mouse-capture configuration. Initial CI was green, but
review found correctness problems in panic restoration and teardown ordering.
The contributor added tests and integrated current `main`; the observed PR still
had a changes-requested review and was behind again after later main activity.[^17]
This is exactly why security-sensitive terminal and process ownership cannot be
delegated to a green badge. The contributor contract should explicitly require
authors to address review findings in the code and explain the result, while the
maintainer remains accountable for the final safety judgment.

### Strict freshness creates visible but justified rework

Pull request 87 was a focused configuration change with green checks, yet it was
behind `main` under the strict required-status policy.[^18] That policy prevents a
green result against an obsolete base from standing in for the merge result. The
cost is another update and CI cycle. GitHub documents this exact tradeoff: strict
checks provide current-base assurance but cause more builds; a merge queue can
reduce rebasing, but it is limited to organization-owned public repositories and
is unjustified at Proqi's volume.[^21][^35]

The repository's update-suggestion setting is off. Enabling it would make the
existing strict policy easier to satisfy, particularly when a fork author has
allowed maintainer edits. The contributor guide should still make clear that
authors normally update their own branches, maintainers may update only with
permission, and a final update can invalidate earlier evidence.

### AI disclosure is already happening, but inconsistently

The recent external PRs inspected here disclosed an AI co-author, an agent, or a
session reference. That transparency is useful, but a full session URL is not
needed to assess a patch and can expose prompts, unrelated context, private
paths, user data, or third-party material. The desired disclosure is small:
tool category, material scope, and confirmation of human review. The desired
evidence remains code, tests, manual behavior, risk reasoning, and the author's
own ability to explain and revise the work.

### Fork workflow approval is not contributor authority

The current `first_time_contributors` setting controls whether GitHub-hosted
workflow execution waits for approval. It does not grant merge, push, review,
secret, or release authority. GitHub warns that once any commit or PR from an
actor is merged, the actor may no longer require workflow approval under the
first-time policies.[^22] A useful first contribution is not evidence that all
future build scripts and tests are safe to execute.

For Proqi, all-external approval is proportionate because the matrix runs
repository code, Rust build scripts, PTY and process tests, package assembly,
Docker builds, and generated-file checks. Approval should mean only “the diff is
safe enough to execute in unprivileged CI,” not “the change is accepted.”
Ordinary `pull_request` jobs remain read-only and secretless for forks, which
limits repository compromise, but does not remove compute abuse, network
exfiltration of public runner data, cache poisoning attempts, or intentionally
pathological tests.[^22][^23][^24]

## 3. Peer evidence

### Selection

The primary peer set contains twelve projects, exceeding the requested minimum
because no single subset covers Proqi's combined concerns:

| Peer | Why comparable | Important difference from Proqi |
|---|---|---|
| Ratatui | Rust terminal UI library and Proqi's rendering ecosystem | Larger multi-maintainer library with a detailed maintainer roster and signed-commit policy.[^45] |
| Crossterm | Rust terminal I/O layer and process-wide terminal behavior | Library surface, feature combinations, and downstream compatibility are broader.[^46] |
| Zellij | Rust terminal multiplexer with PTY, process, plugin, snapshot, and release complexity | Much larger community, published BDFL governance, and constrained roadmap capacity.[^47][^60] |
| Helix | Rust terminal editor with generated docs, integration tests, platform concerns, and an MSRV | Mature editor and language ecosystem with many more contributors.[^48] |
| Alacritty | Rust terminal emulator with escape parsing, platform behavior, reference tests, and security-sensitive input | Larger application, long-lived project, and a blanket LLM-content ban that reflects its maintainers' choice.[^49] |
| WezTerm | Terminal emulator and multiplexer with platform-specific testing and terminal-model invariants | Much broader GUI, graphics, platform, and integration surface.[^50] |
| ripgrep | Prominent Rust CLI with a small-core-maintainer model and a current AI policy | Far narrower interactive state, but a much larger user base and stability history.[^51] |
| fd | Mature Rust CLI with issue-first scope, tests, changelog rules, and explicit AI provenance responsibility | Simpler state and platform boundary than Proqi.[^52] |
| bat | Rust CLI with generated assets, regression fixtures, changelog automation, and terminal output | Generated syntax assets dominate a different review risk.[^53] |
| Nushell | Rust shell and terminal application with platform policy, design discussion, tests, and generated release notes | Large core team, workspace, plugin ecosystem, and formalized release-note flow.[^54] |
| gitui | Rust TUI developer application and direct interaction peer | Its checked-in contributor guidance is much thinner than Proqi's.[^55] |
| lazygit | TUI developer application with a single maintainer explicitly managing review capacity | Go rather than Rust, and its current policy declines ordinary PRs, unlike Proqi's demonstrated willingness to accept bounded work.[^56] |

All peer comparisons use the pinned revisions in Sources. They are pattern
evidence, not a maturity ranking.

### Policy and repository-file comparison

Legend: Y means present in the inspected tree; P means the concern is covered in
another file or only partially; N means no corresponding project-owned file was
found. Enforcement was not inferred from file presence.

| Peer | Contributing | Conduct | Security | Governance or maintainers | PR template | CODEOWNERS | Dependency bot | Changelog or release notes | Distinctive contributor rule |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Ratatui | Y | Y | Y | Y | Y | Y | Y | Y | Discuss nontrivial work, keep PRs small, disclose AI, review every generated line, sign commits. |
| Crossterm | Y | N | N | N | N | Y | Y | Y | CI is the source of truth; stable aggregate check; feature, platform, MSRV, dependency, actionlint, and Zizmor expectations. |
| Zellij | Y | Y | N | Y | N | N | Y | Y | Maintainer capacity limits large work to roadmap items agreed in advance; end-to-end snapshots use containers. |
| Helix | Y | N | N | N | N | N | Y | Y | Generated docs have explicit commands; integration tests encouraged; MSRV fields must move together. |
| Alacritty | Y | N | N | N | Y | N | N | Y | Tests and reference tests expected; all LLM-generated content is prohibited. |
| WezTerm | Y | N | N | N | N | N | Y | P | Terminal behavior tests, platform VM guidance, and docs for changed behavior. |
| ripgrep | P | N | N | N | N | N | N | Y | Human must explain and communicate; autonomous contribution agents and AI-written maintainer replies are prohibited. |
| fd | Y | N | Y | N | N | N | Y | Y | Issue first, disclose AI extent, human must understand, contributor owns copyright and patent provenance. |
| bat | Y | N | Y | N | N | N | Y | Y | Behavior changes update changelog; generated syntax fixtures and regression tests have explicit handling. |
| Nushell | Y | Y | Y | N | Y | N | Y | Y | Core-team discussion before major changes; one functional change; user-facing release-note section; platform policy. |
| gitui | Y | N | N | N | Y | N | Y | Y | Short build and getting-started guidance with good-first-issue routing. |
| lazygit | Y | P | N | N | Y | N | Y | N | Ordinary PRs currently declined to protect single-maintainer review capacity; issues and master testing remain welcome. |

No peer had a dedicated root `SUPPORT` policy at the inspected head. Structured
issue templates or forms existed for Ratatui, Crossterm, Zellij, Helix, WezTerm,
ripgrep, fd, bat, Nushell, gitui, and lazygit. Alacritty's inspected tree did not
contain them. The practical pattern is a visible route for recurring issue types,
not a requirement to create every possible community-health file.[^45][^46][^47][^48][^49][^50][^51][^52][^53][^54][^55][^56]

### Publicly visible peer enforcement

GitHub's public branch metadata and public ruleset endpoints expose only part of
peer enforcement. The detailed legacy protection endpoint was unavailable to
this read-only non-owner inspection for every peer, so the table does not infer
approval counts, bypass actors, or required checks beyond the one public
ruleset GitHub returned.[^61]

| Peer and default branch | Public `protected` field | Public active ruleset | Remote Action references at pinned head |
|---|---:|---|---|
| Ratatui `main` | Yes | `ensure checks pass`: deletion and non-fast-forward blocked, one non-strict required check | 65 of 65 full SHA |
| Crossterm `master` | No | None returned | 28 of 28 full SHA |
| Zellij `main` | No | None returned | 0 of 42 full SHA |
| Helix `master` | Yes | None returned | 8 of 22 full SHA |
| Alacritty `master` | Yes | None returned | 0 of 4 full SHA |
| WezTerm `main` | Yes | None returned | 0 of 192 full SHA |
| ripgrep `master` | Yes | None returned | 1 of 15 full SHA |
| fd `master` | Yes | None returned | 5 of 14 full SHA |
| bat `master` | Yes | None returned | 0 of 19 full SHA |
| Nushell `main` | Yes | None returned | 29 of 29 full SHA |
| gitui `master` | Yes | None returned | 0 of 36 full SHA |
| lazygit `master` | Yes | None returned | 4 of 25 full SHA |

The Action counts come from syntactic inspection of checked-in workflow source,
not a platform setting. Proqi's current all-SHA workflow source is therefore a
strong peer-relative practice, while its disabled platform enforcement remains
a real drift gap.[^45][^46][^47][^48][^49][^50][^51][^52][^53][^54][^55][^56]

Public label inspection showed that most peers use a small combination of
`good first issue` or `help wanted`, documentation, platform, and needs-design,
needs-reproduction, or needs-testing concepts. Larger projects such as Nushell
and WezTerm have deeper state taxonomies. Proqi already has the useful entry and
type labels. Copying a large project's triage state machine would add upkeep
without creating more reviewer capacity.

Three conclusions follow.

First, mature repositories do not converge on a mandatory set of governance
files. Crossterm's technical contributor contract is rigorous despite lacking
root conduct, security, and governance documents at the inspected revision.
Helix similarly emphasizes generated documentation, integration tests, and MSRV
coordination. File-completeness scoring would miss the controls relevant to
their actual risk.[^46][^48]

Second, the strongest peer guidance is specific to review cost. Ratatui asks for
nontrivial discussion, PRs under roughly 500 changed lines where possible, and
separation of refactors from behavior. Zellij and lazygit state maintainer
capacity directly, but choose opposite boundaries: roadmap-aligned large work
only versus no ordinary PRs. Proqi should keep accepting bounded PRs because PR
80 demonstrates value, while preserving the right to stop unaligned work before
implementation.[^45][^47][^56]

Third, generated output needs an owned verification path. Helix regenerates
documentation from source, Zellij compares end-to-end terminal snapshots,
Alacritty uses reference tests, and bat distinguishes generated syntax fixtures
from hand-authored behavior. This supports Proqi's existing rule that snapshots
are evidence to inspect, not files to auto-accept.[^47][^48][^49][^53]

### Common topics with no uniform answer

| Topic | Peer evidence | Implication for Proqi |
|---|---|---|
| Issue first | Ratatui uses it for non-straightforward or large-impact work; fd asks for a ticket before PRs; Zellij requires capacity alignment for large roadmap work; Nushell asks for core-team discussion before major changes | Keep Proqi's scoped rule. Direct typo, docs, narrow bug, and already-approved issue fixes should not need ceremony. |
| PR size | Ratatui gives a reviewability target; lazygit closes the PR path entirely due review cost; others rely on scope language | Prefer coherent small slices and explanations over an inflexible line limit. |
| Tests | Every detailed technical guide expects tests, but platform and fixture forms vary | Keep the canonical gate plus risk-specific evidence. A checklist must allow truthful “not run” reporting. |
| Changelog | fd and bat ask contributors to update it; Nushell derives user-facing release-note material from PRs; Proqi keeps prepared release notes under maintainer control | Ask contributors to describe user-visible impact. Do not grant or impose release-note authority. |
| MSRV | Crossterm and Helix document explicit policies; Zellij explicitly declines an MSRV policy | Proqi already promises 1.88 and tests it. Preserve the promise until an intentional compatibility decision changes it. |
| Commit signing | Ratatui requires it; most inspected peers do not | Do not impose it. It increases contributor setup and bot or rebase complexity without adding a second trusted reviewer. |
| CLA or DCO | No project-sized evidence here shows either is needed for a small MIT app; large foundation or corporate projects use CLAs for their own ownership and patent needs | Preserve inbound-equals-outbound. Seek counsel only for a concrete rights or relicensing concern. |
| Governance | Zellij publishes BDFL governance; Ratatui publishes maintainers; many mature peers publish neither in the inspected repository | Add structure when there are actual delegated roles or contested decisions to describe. |
| Recognition | Changelogs, authorship, release notes, and maintainer lists are common mechanisms | Use ordinary authorship and release notes now. Add a contributor rollup only if omission becomes a real community problem. |

## 4. AI-assisted contributions in 2026

Current respected projects demonstrate three defensible policies, but only one
fits Proqi.

1. **Blanket prohibition.** Alacritty rejects LLM-generated content across code,
   documentation, issues, PRs, and comments.[^49] This creates a bright line but
   is difficult to verify, excludes careful assistance along with low-quality
   automation, and conflicts with Proqi's agent-oriented product and recent
   useful contribution evidence.
2. **Permit with human accountability and disclosure.** Ratatui requires
   attribution, line-by-line review, equal quality, controlled quantity, and
   license compatibility. fd additionally requires disclosure of extent and
   explicit copyright or patent responsibility.[^45][^52] This is closest to
   Proqi's needs.
3. **Permit with human accountability, disclosure optional.** Python's current
   developer guide holds the submitter responsible, requires detailed review
   and explanation in the author's own words, values focused changes and tests,
   and appreciates but does not mandate disclosure.[^58] ripgrep is stricter on
   communication and prohibits autonomous agents, while PostHog requests agent
   context and end-to-end proof rather than treating compilation as evidence.[^51][^59]

The Apache Software Foundation's current legal guidance is helpful but should
not be imported wholesale. It treats generated material like other third-party
material, keeps responsibility with the contributor, recommends recording the
tool, and acknowledges unresolved copyright questions across jurisdictions.[^57]
That supports a provenance duty, not a claim that disclosure cures licensing
risk.

### Recommended Proqi rule

AI assistance is permitted. The person opening the contribution is accountable for it and must:

- understand every material change and be able to explain, test, revise, and
  defend it without delegating the review conversation back to a model;
- review generated code, prose, tests, fixtures, snapshots, and commands under
  exactly the same quality and license standards as human-written material;
- disclose material AI assistance in the PR description by naming the tool
  category and the parts it materially affected;
- verify that they have the right to submit all material and identify copied or
  adapted third-party sources and their licenses;
- provide the same focused, canonical, manual, platform, migration, and snapshot
  evidence the change would require without AI;
- never publish prompts, session transcripts, local paths, credentials, private
  user content, unredacted diagnostics, or generated artifacts that are not
  necessary and reviewed parts of the change; and
- remain the human author of issue and review communication. Language editing
  and translation are acceptable when the final words reflect the contributor's
  own understanding.

Maintainers may close low-signal, untested, unreviewed, oversized, or repetitive
submissions based on their effect on the project, regardless of what tool made
them. This avoids unreliable AI detection and applies one quality standard.

Material disclosure should be required here because all recent external code PRs
already supplied some form of it, the cost is one sentence, and it helps a sole
reviewer calibrate questions. It should not require model versions, prompt logs,
session links, “AI co-author” trailers, or a machine-generated attestation.
Those details create privacy and compliance theater without proving authorship,
rights, understanding, or correctness.

## 5. Threat model

The threat boundary is not “outsider bad, maintainer good.” Public-fork code is
untrusted by default. Trusted-maintainer events have more authority and therefore
need protection against account compromise, mistaken inputs, dependency
compromise, and bypass misuse.

### Public-fork and outside-contributor path

| Threat | Malicious or accidental path | Current mitigation | Gap and proportionate response |
|---|---|---|---|
| Workflow injection through metadata | A branch name, PR title, issue text, commit message, or path is interpolated into a shell command | Ordinary CI mostly uses trusted static commands and passes selected values through environment variables | Continue auditing every `${{ github.event.* }}` use. Never splice untrusted metadata into generated shell. GitHub and OpenSSF both call for validation or safe argument passing.[^23][^41] |
| `pull_request_target` privilege confusion | A privileged base-context workflow checks out or executes fork code | No such trigger found | Retain a prohibition in maintainer guidance. If a write-back workflow is ever needed, split unprivileged build from a privileged metadata-only consumer and treat artifacts as untrusted.[^23] |
| Untrusted Rust build scripts and tests | `build.rs`, procedural macros, test binaries, xtask commands, package hooks, or shell scripts execute arbitrary code on a hosted runner | Fork runs are secretless and token writes are reduced; first-time actors require approval; timeouts bound jobs | Require approval for all external contributors. Reviewer checks workflow, manifest, lockfile, build script, test, fixture, and tool changes before running. Keep hosted, ephemeral runners only. |
| CI denial of service | Infinite tests, huge matrices, decompression bombs, memory or disk exhaustion, repeated synchronizations, or cache churn | Per-job timeouts, cancellation of superseded PR runs, docs-only classification, hosted runners | All-external approval, retain timeouts and concurrency, close abusive PRs, avoid self-hosted runners. Consider repository cache limits only if consumption becomes material. |
| Cache poisoning | Untrusted jobs write entries later restored in trusted jobs | Cache is used in CI; release workflow deliberately has no dependency cache | Never restore PR-created caches into release publication. Keep release cache-free. Review cache key and scope changes as security-sensitive. |
| Secret exfiltration | Code or action reads tokens, environment secrets, credentials, or persisted checkout authentication | Fork `pull_request` token is read-only and secrets are absent; checkout persistence is disabled; default token is read-only | Keep secrets out of PR jobs. Remember runner-accessible values can be transformed beyond automatic redaction. Review every permission increase and secret reference.[^23][^24] |
| Dependency or lockfile substitution | A dependency is added, source changes to Git or another registry, features expand, or a lockfile selects malicious code | Locked graph, cargo-deny source and license policy, cargo-audit, Dependabot grouping, canonical CI | Add change-focused dependency review after measuring overlap. Require rationale for new or major dependencies and explicit lockfile review. Never auto-merge majors. |
| Compromised third-party Action | A mutable tag moves or an action publisher account is compromised | All current actions use full commit SHAs with version comments; Dependabot updates Actions | Turn on full-SHA enforcement and restrict allowed action repositories. Verify that update SHAs belong to the canonical publisher. Full SHA reduces tag movement risk but does not prove the pinned code benign.[^22][^23] |
| Generated snapshot deception | An implementation bug is hidden inside a large accepted snapshot or golden update | Auto-accept forbidden; representative snapshots required; diffs reviewed | PR template must name each generated class, its command, and the human-observed intended change. Keep `.snap.new` failure. |
| Migration corruption | A schema change drops or reinterprets state, fails midway, or creates an incompatible database | Append-only migration ownership, recovery and deterministic tests in architecture | Require old-state upgrade fixtures, idempotence or failure recovery evidence, and explicit data-loss analysis. No destructive migration on inference alone. |
| Path traversal or filesystem escape | Attachment names, import paths, archive members, diagnostics output, or state paths escape owned roots | Typed paths, injected filesystem boundaries, packaging checks, security policy scope | Treat new path parsing and archive extraction as security-sensitive. Test absolute paths, `..`, symlinks, Unicode normalization, and hostile archive names where applicable. |
| Terminal escape or control injection | Thought text, diagnostics, subprocess output, or external metadata emits control sequences or corrupts display state | Terminal normalization, deterministic rendering, control-character and Unicode tests | Require hostile text fixtures for every new render or output boundary. Never trust “looks correct” screenshots alone. |
| Herdr or subprocess command injection | Contributor code converts names or text into shell commands, key injection, or unbounded children | Process adapter boundary, typed protocol values, no-shell product invariant, bounded teardown tests | Changes to command construction and protocol parsing require focused adversarial tests and live read-only qualification where possible. |
| Social engineering | A friendly small PR establishes reputation, followed by a dangerous workflow or dependency change | Owner review and branch rules | Do not equate GitHub contribution history with execution or write trust. Use all-external workflow approval and risk-based review. |
| Maintainer-branch modification surprise | Maintainer pushes to an external author's branch without clear consent, or author changes after approval | GitHub exposes `maintainerCanModify`; some recent PRs enabled it and some did not | Document that contributor consent controls upstream edits. Prefer requested changes; use branch updates only when consent exists and the exact update is reviewable. |

### Trusted-maintainer and publication path

| Threat | Privileged path | Current mitigation | Gap and proportionate response |
|---|---|---|---|
| Maintainer account compromise | Owner bypasses rules, changes workflows or settings, creates protected tags, or adds collaborators | Rulesets make bypass visible in configuration; release pipeline validates exact commits and assets | Require strong GitHub account MFA or passkey outside repository scope. Periodically review collaborators, Apps, deploy keys, secrets, and bypass actors. Do not add write collaborators casually. OpenSSF Level 1 treats MFA and controlled collaborator grants as baseline controls.[^41] |
| Owner bypass mistake | Direct push or bypass skips required review or status | Bypass limited to one owner; repository history remains public | Keep bypass for recovery while single-maintainer, but define it as exceptional. Any bypassed product change should receive immediate full local qualification and public CI on resulting `main`. |
| Trusted event input injection | `workflow_dispatch`, tag, release note, version, or repository variable enters privileged shell | Release plan validates canonical version and tag; shell uses quoted environment values in key places | Audit every new manual input and variable. Allowlist format before shell use. Trusted actor does not make free-form input safe after account takeover.[^41] |
| Package publication | A workflow or collaborator publishes an unintended crate or version | Protected `v*` creation, release environment, exact-main candidate selection, OIDC trusted publishing, post-publication verification | Preserve single tag authorization. Do not give collaborators release or package authority. Consider no-admin-bypass environment review only when a second independent maintainer exists. |
| Artifact substitution | Build output is replaced between candidate, release, crate, Debian, or Homebrew stages | Digest-selected candidate, manifest and byte verification, SHA256 sums, SBOM and provenance attestations, public redownload, enabled immutable releases | Preserve immutability and qualify the draft-first release flow against it. Keep verification instructions visible to users. Attestations link artifact to build identity, not safety.[^33][^34][^64] |
| App token expansion | GitHub App installation or generated token gains access beyond the Homebrew tap | Workflow requests a repository-scoped token and explicit content permission | Review App installation scope and variables periodically. Never replace with a broad personal token. |
| Dependabot auto-merge abuse | A spoofed actor or major update reaches auto-merge, or CI is stale | Workflow checks genuine actor and repository, fetches Dependabot metadata, permits patch or minor only, requests squash auto-merge behind strict status | Preserve exact actor, repository, update-type, strict status, and no-major invariants. Review any trigger or permission expansion. Dependabot events receive fork-like token and secret restrictions by default.[^19][^27] |
| Security embargo leak | Vulnerability details appear in an issue, public fork, log, or ordinary PR | PVR and SECURITY prohibit public disclosure; GitHub advisories offer private collaboration | Keep fixes and reproductions in the advisory's private fork until coordinated disclosure. Publish credit and advisory details only with reporter and maintainer agreement.[^31] |
| Single-maintainer availability | No one can triage a security report or publish a fix if the owner is unavailable | No delegated authority currently | This is real continuity risk, but adding a lightly trusted collaborator is worse. Revisit when a candidate has sustained, reviewed participation and can receive an explicitly scoped role, ideally after organization transfer. |

## 6. Legal and community baseline

### MIT inbound licensing

The current rule is proportionate: contributions accepted into the MIT-licensed
repository are licensed on the same terms. GitHub's Terms state that content
added to a repository with a license is contributed under that license and that
the contributor represents they have the right to do so.[^36] Proqi reinforces
that in both `CONTRIBUTING.md` and the PR template.[^2][^3]

This model leaves contributors owning their contributions while granting the
rights in MIT. It does not prove that an employer, customer, model provider, or
third-party project has granted the contributor those rights. The appropriate
operational control is a clear provenance duty: contributors identify copied or
adapted material, licenses, and generated sources, and maintainers stop review
when ownership is unclear.

### DCO and CLA

The DCO is a per-commit certification that the signer has the right to submit
the contribution under the project's license.[^39] It adds an auditable sign-off,
but also requires trailer correctness across every commit, bot and rewritten
history. It does not investigate whether the certification is true. Proqi
already has explicit inbound terms, one final decision-maker, no foundation
membership model, and no identified patent or relicensing need. Mandatory DCO
does not currently justify its contributor and automation cost.

A CLA is heavier. It is justified where an organization needs additional patent
grants, relicensing rights, corporate contributor administration, or a single
legal agreement across many projects. None is established here. A CLA service
would create identity processing and a merge gate before solving any observed
review problem. Do not adopt one without counsel and a concrete legal objective.

Counsel is warranted if Proqi plans relicensing, dual licensing, transferring IP
to a company or foundation, accepting substantial employer-owned code, resolving
a disputed contribution, incorporating code with incompatible or unclear terms,
or creating contributor patent promises. This report provides no legal
certainty on those questions.

### Commit signing

Signed commits authenticate a key or identity relationship that GitHub can
verify. A required-signature branch rule can block unsigned commits, and
contributors may need to rewrite and force-push history to repair them.[^11]
Ratatui accepts that friction. Most inspected peers do not state the same rule.
For Proqi, signing does not substitute for review, tests, provenance analysis,
or release attestation, and it complicates first contributions, bots, rebases,
and contributor tooling. Do not require it now.

### Conduct, security, and support are different channels

Contributor Covenant 2.1 expects a private contact method and privacy for the
reporter.[^40] GitHub private vulnerability reporting is purpose-built for
security advisories, temporary private forks, embargoed fixes, and vulnerability
credit.[^31] It is not a general conduct inbox. Proqi should provide a private
conduct contact that the owner can reliably receive, state the limits of
confidentiality for a single-person team, and avoid promising response times it
cannot meet.

Support can stay in public issues with no response SLA. The existing latest-only
support statement is clear. A separate support policy becomes useful only after
recurring installation questions, unsupported platforms, old-version requests,
or commercial expectations create measurable ambiguity.

### Attribution, privacy, fixtures, and diagnostics

Git history and PR authorship preserve ordinary attribution. Security advisories
can credit reporters. Third-party code or fixtures need source, license, and any
required notice. REUSE offers comprehensive file-level, machine-readable
copyright and licensing metadata, but adopting it across every file is not
proportionate while Proqi has a single MIT source license and already packages
license and third-party notices.[^44] Revisit only if mixed-license assets,
vendored sources, or downstream compliance requests make file-level ownership
ambiguous.

Fixtures, screenshots, snapshots, databases, logs, terminal captures, and AI
session links can contain prompts, paths, account names, repository names,
tokens, clipboard content, or other user data. The existing issue form and
security policy already require redaction. The contributor contract should
generalize the rule: use synthetic minimal data, inspect generated artifacts,
and never commit or link private user material without explicit authorization.

## 7. Recommended minimal contributor contract

Keep one canonical contract in `CONTRIBUTING.md`. The PR template should prompt
for evidence, not redefine the rules. The following is the minimal content Proqi
should own after a separate documentation implementation ticket.

### Setup and scope

- State Rust 1.88 as the MSRV, identify the current toolchain, and keep
  `cargo xtask doctor`, focused commands, and `cargo xtask check` as the supported
  route.
- Welcome direct PRs for narrow, obvious fixes and documentation.
- Require an issue or maintainer agreement before architecture boundaries,
  durable formats, SQLite migrations, CLI or JSON contracts, public behavior,
  release policy, workflow privileges, platform support, large refactors, or
  major dependencies.
- Explain that prior discussion aligns scope but does not promise merge.
- Ask authors to stop and re-scope when review shows that a patch has become a
  second feature or a broad refactor.

### Architecture and implementation

- Require complete reading of `context/PRODUCT.md` before visible behavior and
  `context/ARCHITECTURE.md` before boundaries or durable contracts.
- Preserve inward dependency direction and canonical owners. Do not add a
  second source of truth to satisfy a local implementation convenience.
- Preserve the 500-line first-party Rust source ceiling, forbidden production
  panic shortcuts, typed identifiers, deterministic ports, bounded teardown,
  and no-shell process construction.
- Separate behavior-preserving refactors from behavior changes unless the
  combination is necessary and explained.

### Evidence

- Run focused tests during development and the complete canonical gate before
  submission. Report exact commands and results. State skipped platform or
  environment checks plainly.
- Add behavior and important-failure-path tests. A passing existing suite is not
  evidence for new behavior by itself.
- For visible TUI work, update representative snapshots in the same commit,
  inspect every affected cell and style, and never auto-accept snapshots.
  Include a screenshot only when it conveys a visual relationship the snapshot
  diff does not.
- For terminal, process, filesystem, clipboard, attachment, and Herdr changes,
  include hostile input, teardown, and supported-platform evidence appropriate
  to the boundary.
- For migrations, show upgrade from the previous durable state, failure or
  recovery behavior, data preservation, and updated fixtures. Never edit an
  already released migration.
- For CLI or JSON changes, name compatibility impact and update current-contract
  fixtures and prepared release notes when the pre-1.0 contract intentionally
  changes.
- For generated files, state the canonical source, generator command, and human
  review performed. Commit only required outputs.

### Commit and PR conduct

- Use a real contributor identity and an email choice the contributor intends
  to publish through Git history. A GitHub noreply address is acceptable.
- Keep commits buildable and focused enough to review. Maintainers own the final
  squash or rebase decision and may ask for history cleanup.
- In the PR body, link the issue or prior decision when required, explain user
  impact and architecture impact, identify risk areas, list exact verification,
  identify generated, snapshot, migration, dependency, lockfile, workflow, or
  release-input changes, and disclose material AI assistance.
- Do not claim tests, platforms, screenshots, or manual behavior that were not
  actually verified.
- Respond to review in the contributor's own words. Fix the underlying issue,
  explain disagreement with evidence, and do not mark a thread resolved until
  the concern is addressed or explicitly accepted.
- Keep the branch current with `main` for final CI. Authors normally update
  their branch. A maintainer may do so only when branch modification is allowed
  and the update is clearly within the accepted contribution.

### Authority and safety

- Opening a PR grants no merge, collaborator, package, tag, release, issue
  moderation, or repository-setting authority. Maintainers decide acceptance,
  final history, and release inclusion.
- Report vulnerabilities only through the private route in `SECURITY.md`.
  Do not publish an exploit, suspected secret, or embargoed fix in an issue,
  ordinary PR, discussion, CI log, or AI session.
- Never commit credentials, `.env` data, runtime databases, private prompts,
  clipboard content, unredacted diagnostics, personal file paths, user captures,
  or unrelated local state. Use synthetic fixtures.
- Identify third-party material and its license. Confirm the right to submit it.
- Permit AI assistance under the human-accountability rule in section 4.

## 8. Recommended GitHub controls

### Immediate no-regret controls to preserve

| Control | Exact setting | Rationale |
|---|---|---|
| Main required status | Keep `Required CI result`, `strict_required_status_checks_policy: true` | One stable aggregate owner prevents matrix-name drift and proves the tested merge base is current. |
| Main history safety | Keep PR required, deletion blocked, non-fast-forward blocked, squash and rebase only | Prevents accidental destructive history and keeps final history reviewable. |
| Owner bypass | Keep the single owner as bypass actor while there is no second maintainer; document exceptional use | Removing it could strand a one-person project during rule or CI failure. Adding more bypass actors would expand the compromise path. |
| Default token | Keep read-only default and `can_approve_pull_request_reviews: false` | Least privilege, with explicit write scopes only in bounded workflows.[^22][^23] |
| Fork secret boundary | Keep ordinary validation on `pull_request`; do not send secrets or write tokens to forks | GitHub strips write permissions and secrets for ordinary fork PR runs.[^22] |
| PVR and secret prevention | Keep PVR, secret scanning, and push protection enabled | Clear private intake and early credential blocking. Push protection is not exhaustive, so policy remains necessary.[^30][^31] |
| Release boundary | Keep `release` environment restricted to `v*.*.*`, protected `v*` tags, exact candidate selection, OIDC publishing, SBOMs, attestations, and public-byte verification | Separates untrusted build and ordinary merge activity from publication. |
| Immutable releases | Keep the repository setting enabled and verify each release path remains compatible | Prevents published asset and tag substitution. The REST endpoint confirms the current enabled state; operational compatibility still needs qualification before the next release.[^33][^64] |
| Dependabot | Keep weekly grouped updates and patch or minor squash auto-merge behind strict CI; keep majors manual | Bounded automation reduces maintenance delay without delegating breaking-change judgment.[^19][^27] |

### Immediate hardening that needs a settings ticket

| Control | Exact recommendation | Risk reduction or benefit | Friction and caveat |
|---|---|---|---|
| Full-SHA enforcement | Enable **Require actions to be pinned to a full-length commit SHA** | Makes the existing checked-in convention a platform rule. GitHub identifies full SHA as the only immutable action reference.[^23] | Reusable workflows are not covered by the same rule. Dependabot updates must remain reviewed. |
| Allowed Actions | Select “Allow owner, and select non-owner, actions and reusable workflows.” Allow local actions and the external repositories currently used: `actions/checkout@*`, `actions/upload-artifact@*`, `actions/download-artifact@*`, `actions/attest@*`, `actions/create-github-app-token@*`, `dependabot/fetch-metadata@*`, `Swatinem/rust-cache@*`, `taiki-e/install-action@*`, `docker/setup-buildx-action@*`, `docker/login-action@*`, `docker/build-push-action@*`, `anchore/sbom-action@*`, and `rust-lang/crates-io-auth-action@*`. Add `github/codeql-action@*` only if the proposed CodeQL pilot proceeds. | Prevents an approved workflow edit from silently introducing an arbitrary action source. | Every legitimate new action needs a settings update. Review the list against current `main` in the implementation ticket.[^22] |
| Fork approval | Require approval for **all external contributors** | Prevents one benign merged change from becoming permanent permission to execute later untrusted CI. | Adds one maintainer action to every external run and synchronization. At current volume that is smaller than the review risk. |
| Conversation resolution | Set `required_review_thread_resolution: true` | Prevents merge while a concrete review concern remains visibly open. | Authors and maintainers must resolve threads deliberately. It does not require an approval. |
| Update suggestion | Enable **Always suggest updating pull request branches** | Reduces strict-freshness confusion and gives write-capable maintainers an explicit UI path where allowed.[^21] | Outside authors without write access still control their forks; maintainer update depends on their branch permission. Document consent first. |
| CodeQL pilot | Enable default setup for Rust and Actions, default query suite, standard hosted runner; do not add it to required status yet | Low-maintenance SAST for source and workflow patterns, including dangerous Actions constructs.[^23][^29] | Default setup excludes fork PRs and can produce false positives. Review results for several weeks before deciding on enforcement. |

The action allowlist should use repository patterns with `@*`, while the separate
SHA setting enforces each actual reference. This avoids encoding every dependency
update SHA in repository settings while still limiting publishers. GitHub permits
specific action and reusable-workflow patterns and always permits local actions
under the selected-actions mode.[^22]

### Owner choices before changing enforcement

| Choice | Recommendation | Why it is a real choice |
|---|---|---|
| All-external workflow approval | Adopt now, reassess after sustained volume | Security and compute control outweigh one-click latency today. A maintainer could instead accept repeat-contributor execution risk to reduce waiting. |
| Update-branch suggestion and maintainer edits | Enable with explicit contributor-language | It reduces freshness friction, but maintainers should not rewrite a fork branch without consent. |
| CodeQL query suite | Start with default, not security-extended and not required | The repository needs observed signal before accepting more findings or merge blockers. |
| Release environment reviewer | Keep none while only one maintainer; add a non-self reviewer only when a genuinely independent release maintainer exists | A sole owner cannot provide independent approval. A nominal reviewer creates availability risk without separation of duty. |
| Owner bypass | Keep now, narrow or remove after a second trusted maintainer can recover rules safely | It trades continuity against account-compromise impact. |
| Repository ownership | Stay personal now; move to an organization before delegating triage, maintain, or admin duties | A personal repository offers only owner and write collaborator roles. Organization roles make least-privilege delegation possible.[^26] |

### Controls to defer or avoid

| Control | Verdict | Trigger to revisit |
|---|---|---|
| One required approval | Defer | A second maintainer can reliably review owner and contributor work. |
| Code-owner approval | Defer | Distinct maintainers own distinct high-risk paths. With `* @oborchers`, it duplicates sole-owner merge control. |
| Dismiss stale approvals and last-push approval | Defer | Approvals become required and multiple trusted authors can push to a PR. |
| Multiple approvals | Avoid at current scale | A stable reviewer pool larger than two and a demonstrated high-risk change volume. |
| Mandatory signed commits | Avoid | A concrete impersonation or compliance requirement outweighs bot, rebase, and contributor setup friction. |
| Merge queue | Unavailable and unnecessary | Transfer to an organization plus enough concurrent green PRs that strict freshness becomes a throughput problem.[^35] |
| General auto-merge | Avoid | Mature multi-maintainer review automation with reliable labels and ownership. Preserve only bounded Dependabot automation. |
| `pull_request_target` write-back | Avoid | A necessary metadata-only workflow has a reviewed privilege-separation design. Never execute fork code in it.[^23] |
| Required CodeQL | Defer | Default setup has a stable low-noise history and fork coverage limitations are understood. |
| OpenSSF Scorecard badge or hard score target | Defer | Users or downstream distributors need a public posture signal. Scorecard is a heuristic, not the owner of Proqi's risk model.[^42] |
| Formal SLSA level claim | Avoid for now | A release consumer requires a documented conformance claim and each requirement is independently verified. Existing attestations can be useful without a marketing claim.[^34][^43] |
| REUSE across all files | Avoid for now | Mixed licenses, vendored source, or downstream compliance needs make file-level declarations materially useful.[^44] |

## 9. Contributor permissions and trust

Proqi is owned by a personal GitHub account. GitHub exposes only two repository
permission levels in that topology: owner and collaborator. A collaborator can
push, manage issues and labels, merge PRs, affect required reviews, create or
edit releases, and publish packages. This is far broader than triage.[^26]

GitHub package permission does not itself grant crates.io ownership. crates.io
owners and trusted publishers are configured separately. A published crate
version cannot be overwritten or deleted, and a named crates.io owner can
publish, yank, and manage other owners, so that authority should remain at least
as restricted as the release environment.[^62][^63]

| Actor | Current effective capability | What they cannot do by default | Recommendation |
|---|---|---|---|
| Anonymous visitor | Read public code, issues, PRs, releases, checks, and policies | Write content or run an authenticated contribution flow | No special treatment. |
| Outside contributor | Fork, open issue or PR, push to own branch, comment, submit a non-binding review, view CI | Push upstream, merge, change settings, read secrets, publish, create protected tags | Keep outside by default. Evaluate code and behavior, not profile or popularity. |
| First-time contributor | Same repository rights as any outsider; fork workflow waits for approval under current setting | No extra repository authority | Change execution policy to all external, making this distinction irrelevant for CI safety. |
| Repeat contributor | Same repository rights as any outsider; current workflow approval may be skipped after a merged contribution | No collaborator rights merely from contribution history | Do not equate repetition with trusted execution or write access. |
| Public reviewer | Can submit review comments | A review without write access does not satisfy a required approval | Welcome useful review, but maintainers verify it independently. |
| Collaborator on this personal repo | Broad write capability, including push, merge, issue management, releases, packages, and code-owner eligibility | Owner-only settings and ownership actions | Do not grant for triage. Grant only when the person is trusted with code, repository mutation, and release-adjacent consequences. |
| Triager or writer | No separately grantable personal-repo role | Cannot receive limited role without topology change | If needed, transfer to an organization and use Triage for issue work, Write only for active code maintainers. |
| Maintainer | Project role, not a current separate GitHub grant | No implicit platform power | Define responsibilities before appointing one. Prefer sustained review, architecture understanding, security judgment, reliable communication, and least privilege. |
| Repository owner | Full control and current ruleset bypass, release and security-advisory administration | No independent reviewer or recovery person exists | Protect account strongly, use bypass exceptionally, audit integrations, and plan continuity before contributor growth makes it urgent. |
| GitHub App | Only installation and token permissions granted, but a workflow may mint a scoped token | No authority outside installation and requested scopes | Keep installations minimal. Review repository selection, permissions, variables, and token creation workflow periodically. |
| Dependabot | Opens dependency PRs and can trigger workflows under fork-like restrictions; current trusted workflow may request bounded auto-merge | No general release or settings authority | Preserve actor, repository, metadata, semver, and strict-CI guards. |
| Ordinary Actions job | Current default read token, further narrowed or elevated by workflow | No secrets on external fork `pull_request`; cannot approve PR reviews under current setting | Treat every job permission and secret as code review surface. Keep write jobs off untrusted code paths. |
| Release Actions job | Write contents, attestations and artifact metadata, OIDC identity, environment secrets, scoped App token | Runs only from protected stable tag and environment policy | Keep this the sole publication principal. Never expose it to PR or issue-controlled execution. |

### When to elevate trust

Do not promote someone because they opened a fixed number of PRs. Consider
elevation only after sustained evidence across several dimensions:

1. They repeatedly make or review scoped changes that preserve architecture and
   durable contracts.
2. They report tests and uncertainty truthfully, including platform gaps and
   failed approaches.
3. They recognize security-sensitive boundaries, protect user data, and do not
   trade invariants for a green test.
4. They respond constructively and can explain code independently of an AI tool.
5. They remain available long enough to finish review and support consequences.
6. There is recurring work that actually needs delegated authority.

Before the first elevation, decide whether to move the repository to an
organization. That permits a Triage role for issue management without source
write, a Maintain role for day-to-day management without all admin powers, team
CODEOWNERS, and a clearer offboarding path. Repository transfer, role grants,
and settings changes are owner decisions outside this spike.

## 10. Prioritized roadmap

### Now

| Item | Expected risk reduction or contributor benefit | Effort | Ongoing burden | False-positive or friction risk | Dependencies | Acceptance criteria |
|---|---|---:|---:|---:|---|---|
| Contributor contract amendments | Prevents unscoped work, false evidence, unsafe fixtures, migration ambiguity, and AI handoff confusion | S | Low | Low | Owner approves policy wording | One canonical guide covers every topic in section 7 without duplicating architecture contracts. |
| PR template amendments | Makes issue context, exact checks, skipped platforms, risk surfaces, generated files, and AI use visible at review start | S | Low | Low | Contributor contract wording | Template is concise, every checkbox has an owner, and docs-only PRs can mark non-applicable items honestly. |
| Private conduct contact | Gives Covenant reports a usable route that is not a security advisory | S | Low | Low | Owner selects a monitored private channel and realistic confidentiality wording | Code of Conduct names the route, responsible person, scope, and no false SLA. |
| Actions SHA enforcement and allowlist | Prevents unpinned or arbitrary third-party Actions from entering later | S | Low | Medium when adding actions | Exact current action inventory and owner settings approval | Repository reports SHA enforcement on; selected action list matches current `main`; every workflow remains green. |
| All-external fork approval | Prevents contribution history from silently becoming CI execution trust | S | Medium per external update | Medium | Owner accepts approval latency | API reports `all_external_contributors`; contributor docs explain approval is execution screening, not acceptance. |
| Conversation resolution | Prevents known review concerns from remaining open at merge | S | Low | Low | Owner settings approval | Main ruleset requires resolution and a test PR proves the expected merge block. |
| CodeQL pilot | Adds SAST for Rust and Actions without an immediate merge gate | S | Low | Medium alert noise | Owner settings approval | Default setup enabled for Rust and Actions; initial results triaged; no required status added during pilot. |
| Immutable-release flow qualification | Preserves protection against future asset and tag substitution without assuming the existing draft flow is compatible | S | Low | Low if draft flow is correct | Review release reconciliation, recovery, and deletion behavior | Read-only inspection records the setting as enabled; a dry review documents compatibility; the next release publishes all assets before immutability locks it.[^64] |

### Next

| Item | Expected risk reduction or contributor benefit | Effort | Ongoing burden | False-positive or friction risk | Dependencies | Acceptance criteria |
|---|---|---:|---:|---:|---|---|
| Dependency review trial | Highlights newly introduced vulnerabilities and license changes at PR time | S | Low | Medium overlap with Cargo gates | Baseline existing checks; pinned action and allowlist update | Several dependency PRs compared; keep only if it finds useful change-specific evidence; aggregate result remains stable. |
| Maintainer security runbook | Reduces account, App, secret, bypass, and release recovery ambiguity | M | Quarterly review | Low | Owner chooses private versus public portions | Inventory of collaborators, Apps, deploy keys, environments, secrets by purpose, bypass actors, rotation and incident actions, with no secrets stored in docs. |
| Contributor branch and freshness guidance | Reduces repeated behind-main confusion while preserving strict status | S | Low | Low | Update suggestion decision | Guide explains contributor update responsibility, maintainer modification consent, and when CI reruns. |
| Security response refinement | Makes acknowledgment, embargo, credit, supported versions, and publication decisions repeatable | S | Low | Risk of unrealistic promises | Owner chooses nonbinding response wording | SECURITY retains no false SLA, names triage steps, safe private collaboration, and advisory publication criteria. |
| Good-first-issue quality criteria | Prevents newcomers from selecting work that requires hidden product or architecture judgment | S | Low | Low | Enough candidate issues | Label means bounded scope, acceptance evidence, no security-sensitive hidden dependency, and available maintainer context. |

### Defer

| Item | Expected later value | Effort | Ongoing burden | Friction risk | Dependency or trigger | Acceptance criteria when triggered |
|---|---|---:|---:|---:|---|---|
| Organization transfer and granular roles | Least-privilege triage and maintain delegation, continuity | M | Medium | Medium governance and migration cost | A second recurring maintainer or triager needs authority | Ownership, package, App, secrets, branch and release effects reviewed; roles assigned minimally; rollback and communication planned. |
| Maintainer charter and roster | Makes authority, promotion, removal, recusal, and release responsibility explicit | M | Medium | Medium ceremony | At least two active maintainers or recurring authority questions | Names current maintainers, scopes, decision method, inactivity and removal path, security and release responsibilities. |
| Required non-author approval | Protects owner and contributor changes with independent review | S | High availability cost | High with a tiny team | Two dependable reviewers | Every product change can receive timely qualified review; emergency bypass is defined and audited. |
| Code-owner review by path | Routes high-risk changes to real specialists | M | Medium | High if ownership is nominal | Distinct maintainers actually own workflows, migrations, terminal/process, and release | CODEOWNERS maps real responsibility; required owners have write access and backups; stale approval policy defined. |
| Contributor recognition program | Improves retention and makes community work visible | S | Medium | Low | Sustained contributor volume | Release-note or periodic acknowledgment has a named owner and does not expose unwanted personal information. |
| Dedicated support policy | Sets response and version boundaries at scale | S | Medium | Low | Repeated support ambiguity | Separates bugs, usage questions, security, versions, platforms, and no-SLA expectations without duplicating README. |

Size key: S is a bounded file or settings change with focused verification. M
requires coordinated policy, topology, or operational review.

## 11. Separate implementation slices

No slice is implemented by this report.

1. **Contributor documentation.** Amend `CONTRIBUTING.md` with review response,
   branch freshness, migration evidence, generated-file provenance, private-data
   rules, and final merge authority. This can proceed in parallel with the
   security and GitHub hardening slices after owner approval of the principles.
2. **Issue and PR intake.** Tighten the existing PR template and only those issue
   form prompts that map directly to the approved contributor contract. Do not
   add forms or labels without a demonstrated route. Depends on slice 1 wording,
   but implementation can be prepared concurrently.
3. **AI-assisted contribution policy.** Add the compact section recommended in
   section 4, preferably inside `CONTRIBUTING.md`, not a separate framework or
   machine-readable standard. Requires owner choice on mandatory versus
   encouraged material disclosure. Recommendation: mandatory one-sentence
   material disclosure, no prompt or session publication.
4. **Security and conduct reporting.** Preserve PVR for vulnerabilities, improve
   response-process wording, and add a separate private conduct route. Requires
   the owner to select the conduct channel and approve accurate confidentiality
   and availability language.
5. **GitHub Actions hardening.** Enable full-SHA enforcement, configure the exact
   action allowlist, change fork approval to all external, and verify every
   workflow. This can proceed independently once the owner approves the added
   approval latency and allowlist maintenance.
6. **Branch and analysis controls.** Require conversation resolution, enable
   update suggestions, and start a non-required CodeQL default-setup pilot.
   Requires owner choices identified in section 8. Preserve strict freshness.
7. **Release immutability qualification.** Preserve the enabled setting, review
   current draft reconciliation and recovery, then qualify the next release
   flow. This is separate from general Actions hardening because publication
   recovery has a different failure cost.
8. **Dependency review experiment.** Add a pinned, least-privilege dependency
   review job, compare it with `cargo audit` and `cargo deny`, and retain it only
   if it adds actionable change-specific signal. Can run in parallel with the
   CodeQL pilot after the Action allowlist can admit it.
9. **Maintainer governance and topology.** Deferred until a second person needs
   authority. Decide organization transfer, roles, ownership, approval count,
   stale review behavior, bypass, continuity, and removal together. Do not grant
   broad personal-repository collaborator access as a shortcut.

Parallel group A: slices 1, 4, and 5 can proceed independently after their
specific owner choices. Slice 2 follows the approved language from slice 1.
Parallel group B: slices 6, 7, and 8 are independent experiments or settings
changes, but each needs separate owner authorization and rollback evidence.
Slice 9 must wait for a real delegation need.

## 12. Direct answer

**Tighten now:** material AI accountability and privacy, PR evidence prompts,
private conduct reporting, all-external fork run approval, Action allowlisting
and SHA enforcement, conversation resolution, contributor-friendly strict-base
updates, a non-required CodeQL pilot, and focused qualification of the release
flow against the already-enabled immutable-release setting.

**Keep as-is:** the scoped issue-first rule, MIT inbound-equals-outbound, no CLA
or DCO, Rust 1.88 policy, canonical gate, snapshot review, strict current-`main`
required status, protected `v*` tags, owner-controlled publication, private
vulnerability reporting, secret scanning and push protection, exact artifact
verification, immutable releases, and bounded Dependabot patch and minor
auto-merge.

**Wait for scale:** organization transfer, triage and maintain roles, formal
maintainer promotion, code-owner enforcement, required approvals, stale approval
dismissal, contributor recognition programs, and a separate support policy.

**Avoid:** governance documents without delegated governors, a blanket issue
prerequisite, blanket AI prohibition, prompt transcript collection, signed-commit
mandates, multiple approvals in a one-maintainer project, broad collaborator
grants for triage, general auto-merge, privileged fork execution, a merge queue,
and framework badges or maturity claims that are not tied to Proqi's threat model.

The professional baseline is not the number of policy files. It is a truthful
contract, a safe untrusted-contribution path, a reviewer who owns the decision,
and a publication boundary that contributors cannot cross accidentally.

## Sources

All web sources were accessed on 2026-09-09. Repository-file links are pinned to
the inspected commit unless a live API endpoint is specifically identified.

[^1]: Proqi maintainers, “Repository tree at the research baseline,” commit `8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea`, [GitHub](https://github.com/oborchers/proqi/tree/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea).
[^2]: Proqi maintainers, “Contributing to Proqi,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/CONTRIBUTING.md).
[^3]: Proqi maintainers, “Pull request template,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/.github/pull_request_template.md).
[^4]: Proqi maintainers, “Contributor Covenant Code of Conduct,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/CODE_OF_CONDUCT.md).
[^5]: Proqi maintainers, “Security Policy,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/SECURITY.md).
[^6]: Proqi maintainers, “Issue templates,” baseline commit, [GitHub](https://github.com/oborchers/proqi/tree/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/.github/ISSUE_TEMPLATE).
[^7]: Proqi maintainers, “Cargo manifest,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/Cargo.toml).
[^8]: Proqi maintainers, “GitHub Actions workflows,” baseline commit, [GitHub](https://github.com/oborchers/proqi/tree/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/.github/workflows).
[^9]: Proqi maintainers, “Release runbook,” baseline commit, [GitHub](https://github.com/oborchers/proqi/blob/8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea/docs/RELEASING.md).
[^10]: GitHub REST API, “Get a repository,” owner-authenticated read-only inspection of public Proqi settings, [API endpoint](https://api.github.com/repos/oborchers/proqi).
[^11]: GitHub REST API, “Repository rulesets,” read-only inspection of Proqi branch and tag rules, [API endpoint](https://api.github.com/repos/oborchers/proqi/rulesets).
[^12]: GitHub REST API, “GitHub Actions permissions,” owner-authenticated read-only inspection, [API endpoint](https://api.github.com/repos/oborchers/proqi/actions/permissions). The workflow-permission and fork-approval subresources were also inspected.
[^13]: GitHub REST API, “Release environment,” owner-authenticated read-only inspection, [API endpoint](https://api.github.com/repos/oborchers/proqi/environments/release).
[^14]: GitHub REST API, “Code scanning default setup,” owner-authenticated read-only inspection, [API endpoint](https://api.github.com/repos/oborchers/proqi/code-scanning/default-setup).
[^15]: Proqi pull request 66, “Add thurbox as a second agent submission backend,” opened 2026-09-07, closed unmerged 2026-09-08, [GitHub](https://github.com/oborchers/proqi/pull/66).
[^16]: Proqi pull request 80, “Support Herdr protocol 21 and provisional protocol 22,” opened and merged 2026-09-08, [GitHub](https://github.com/oborchers/proqi/pull/80).
[^17]: Proqi pull request 81, “Add mouse_capture setting to disable mouse capture,” opened 2026-09-08, state inspected at cutoff, [GitHub](https://github.com/oborchers/proqi/pull/81).
[^18]: Proqi pull request 87, “Add footer_hidden config setting to hide the footer permanently,” opened 2026-09-08, state inspected at cutoff, [GitHub](https://github.com/oborchers/proqi/pull/87).
[^19]: Proqi pull request 88, “Automate safe Dependabot updates,” merged 2026-09-08, including the bounded auto-merge workflow at commit `a00f55da09d8ff8922ea1cbf04ea00a0b9662acd`, [GitHub](https://github.com/oborchers/proqi/pull/88).
[^20]: GitHub, “About rulesets,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets).
[^21]: GitHub, “About protected branches,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).
[^22]: GitHub, “Managing GitHub Actions settings for a repository,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository).
[^23]: GitHub, “Secure use reference,” current documentation, [GitHub Docs](https://docs.github.com/en/actions/reference/security/secure-use).
[^24]: GitHub, “Compromised runners,” current documentation, [GitHub Docs](https://docs.github.com/en/actions/concepts/security/compromised-runners).
[^25]: GitHub, “About code owners,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
[^26]: GitHub, “Permission levels for a personal account repository,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/repository-access-and-collaboration/permission-levels-for-a-personal-account-repository).
[^27]: GitHub, “Dependabot on GitHub Actions,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-on-actions).
[^28]: GitHub, “Dependency review,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/concepts/supply-chain-security/dependency-review).
[^29]: GitHub, “Configuring default setup for code scanning,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/configure-code-scanning/configure-code-scanning).
[^30]: GitHub, “Secret scanning detection scope,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/reference/secret-security/secret-scanning-scope).
[^31]: GitHub, “Coordinated disclosure of security vulnerabilities” and “Privately reporting a security vulnerability,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/concepts/vulnerability-reporting-and-management/coordinated-disclosure), [reporting guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/report-privately).
[^32]: GitHub, “Deployments and environments,” current documentation, [GitHub Docs](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).
[^33]: GitHub, “Immutable releases” and “Preventing changes to your releases,” current documentation, [GitHub Docs](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [configuration guide](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes).
[^34]: GitHub, “Artifact attestations,” current documentation, [GitHub Docs](https://docs.github.com/en/actions/concepts/security/artifact-attestations).
[^35]: GitHub, “Managing a merge queue,” current documentation, [GitHub Docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
[^36]: GitHub, “GitHub Terms of Service,” sections D.5 and D.6, current terms, [GitHub Docs](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service).
[^37]: The Rust Project, “The `rust-version` field,” current Cargo reference, [doc.rust-lang.org](https://doc.rust-lang.org/cargo/reference/rust-version.html).
[^38]: The Rust Project, “Cargo.toml vs Cargo.lock,” current Cargo guide, [doc.rust-lang.org](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html).
[^39]: Developer Certificate of Origin, “Version 1.1,” current text, [developercertificate.org](https://developercertificate.org/).
[^40]: Organization for Ethical Source, “Contributor Covenant Code of Conduct, version 2.1,” [Contributor Covenant](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).
[^41]: Open Source Security Foundation, “Open Source Project Security Baseline,” version 2026.02.19, [OpenSSF](https://baseline.openssf.org/versions/2026-02-19).
[^42]: Open Source Security Foundation, “OpenSSF Scorecard,” current project documentation, [scorecard.dev](https://scorecard.dev/).
[^43]: SLSA community, “SLSA specification,” version 1.1, [slsa.dev](https://slsa.dev/spec/v1.1/).
[^44]: Free Software Foundation Europe, “REUSE Specification,” version 3.3, 2024-11-14, [reuse.software](https://reuse.software/spec/).
[^45]: Ratatui maintainers, “Contribution guidelines” and maintainer roster, commit `99168f8afa2d75f89beeee17f0d15835010177c4`, [CONTRIBUTING.md](https://github.com/ratatui/ratatui/blob/99168f8afa2d75f89beeee17f0d15835010177c4/CONTRIBUTING.md), [repository tree](https://github.com/ratatui/ratatui/tree/99168f8afa2d75f89beeee17f0d15835010177c4).
[^46]: Crossterm maintainers, “Contributing,” commit `cdc30a9cd7f89d97a847c182feab66147eb50223`, [CONTRIBUTING.md](https://github.com/crossterm-rs/crossterm/blob/cdc30a9cd7f89d97a847c182feab66147eb50223/docs/CONTRIBUTING.md), [repository tree](https://github.com/crossterm-rs/crossterm/tree/cdc30a9cd7f89d97a847c182feab66147eb50223).
[^47]: Zellij maintainers, “Contributing to Zellij,” commit `af38660c5884f50bb3726682fb92961326c4268f`, [CONTRIBUTING.md](https://github.com/zellij-org/zellij/blob/af38660c5884f50bb3726682fb92961326c4268f/CONTRIBUTING.md), [repository tree](https://github.com/zellij-org/zellij/tree/af38660c5884f50bb3726682fb92961326c4268f).
[^48]: Helix maintainers, “Contributing,” commit `079a789e8cb08ead67f19e1971a1b7438b37354b`, [CONTRIBUTING.md](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/docs/CONTRIBUTING.md), [repository tree](https://github.com/helix-editor/helix/tree/079a789e8cb08ead67f19e1971a1b7438b37354b).
[^49]: Alacritty maintainers, “Contributing to Alacritty,” commit `d692748d3f61253ebe9f5094320120d22f6a046f`, [CONTRIBUTING.md](https://github.com/alacritty/alacritty/blob/d692748d3f61253ebe9f5094320120d22f6a046f/CONTRIBUTING.md), [repository tree](https://github.com/alacritty/alacritty/tree/d692748d3f61253ebe9f5094320120d22f6a046f).
[^50]: WezTerm maintainers, “Contributing,” commit `d2f3f05b38f26a872f4b0bfbb3d2eaa7bdfc1b0b`, [CONTRIBUTING.md](https://github.com/wezterm/wezterm/blob/d2f3f05b38f26a872f4b0bfbb3d2eaa7bdfc1b0b/CONTRIBUTING.md), [repository tree](https://github.com/wezterm/wezterm/tree/d2f3f05b38f26a872f4b0bfbb3d2eaa7bdfc1b0b).
[^51]: ripgrep maintainers, “AI Policy,” commit `3fce3b5bb0236da2df6d99672afb8a719642eca7`, [AI_POLICY.md](https://github.com/BurntSushi/ripgrep/blob/3fce3b5bb0236da2df6d99672afb8a719642eca7/AI_POLICY.md), [repository tree](https://github.com/BurntSushi/ripgrep/tree/3fce3b5bb0236da2df6d99672afb8a719642eca7).
[^52]: fd maintainers, “Contributing to fd,” commit `bb80489efb9fe2bdeabe1844feb5ee0af6fc9b1c`, [CONTRIBUTING.md](https://github.com/sharkdp/fd/blob/bb80489efb9fe2bdeabe1844feb5ee0af6fc9b1c/CONTRIBUTING.md), [repository tree](https://github.com/sharkdp/fd/tree/bb80489efb9fe2bdeabe1844feb5ee0af6fc9b1c).
[^53]: bat maintainers, “Contributing,” commit `7323a7514f7601737640e7172be115127d6db08c`, [CONTRIBUTING.md](https://github.com/sharkdp/bat/blob/7323a7514f7601737640e7172be115127d6db08c/CONTRIBUTING.md), [repository tree](https://github.com/sharkdp/bat/tree/7323a7514f7601737640e7172be115127d6db08c).
[^54]: Nushell maintainers, “Contributing,” commit `9d3157963241cf89447119d34d6e887859f5e7e8`, [CONTRIBUTING.md](https://github.com/nushell/nushell/blob/9d3157963241cf89447119d34d6e887859f5e7e8/CONTRIBUTING.md), [repository tree](https://github.com/nushell/nushell/tree/9d3157963241cf89447119d34d6e887859f5e7e8).
[^55]: GitUI maintainers, “Contributing,” commit `2fa693cb6ed431b21ebc300dd02e83c2476699ce`, [CONTRIBUTING.md](https://github.com/gitui-org/gitui/blob/2fa693cb6ed431b21ebc300dd02e83c2476699ce/CONTRIBUTING.md), [repository tree](https://github.com/gitui-org/gitui/tree/2fa693cb6ed431b21ebc300dd02e83c2476699ce).
[^56]: lazygit maintainers, “Contributing,” commit `c07f4d381b90419583b7ce04f87379654d983ebc`, [CONTRIBUTING.md](https://github.com/jesseduffield/lazygit/blob/c07f4d381b90419583b7ce04f87379654d983ebc/CONTRIBUTING.md), [repository tree](https://github.com/jesseduffield/lazygit/tree/c07f4d381b90419583b7ce04f87379654d983ebc).
[^57]: Apache Software Foundation, “Generative Tooling Guidance,” current guidance, [apache.org](https://www.apache.org/legal/generative-tooling.html).
[^58]: Python core developers, “Guidelines for using AI tools,” current Python Developer's Guide, [python.org](https://devguide.python.org/contrib/project/generative-ai/).
[^59]: PostHog maintainers, “AI contributions policy,” pinned revision inspected at cutoff, [GitHub](https://github.com/PostHog/posthog/blob/1732fba27a5e7cfe9aa0d3391de5c26492bde759/AI_POLICY.md).
[^60]: Zellij maintainers, “Governance,” commit `af38660c5884f50bb3726682fb92961326c4268f`, [GitHub](https://github.com/zellij-org/zellij/blob/af38660c5884f50bb3726682fb92961326c4268f/GOVERNANCE.md).
[^61]: GitHub REST API, public default-branch and ruleset inspection for the twelve peer repositories at the pinned heads. Direct branch endpoints: [Ratatui](https://api.github.com/repos/ratatui/ratatui/branches/main), [Crossterm](https://api.github.com/repos/crossterm-rs/crossterm/branches/master), [Zellij](https://api.github.com/repos/zellij-org/zellij/branches/main), [Helix](https://api.github.com/repos/helix-editor/helix/branches/master), [Alacritty](https://api.github.com/repos/alacritty/alacritty/branches/master), [WezTerm](https://api.github.com/repos/wezterm/wezterm/branches/main), [ripgrep](https://api.github.com/repos/BurntSushi/ripgrep/branches/master), [fd](https://api.github.com/repos/sharkdp/fd/branches/master), [bat](https://api.github.com/repos/sharkdp/bat/branches/master), [Nushell](https://api.github.com/repos/nushell/nushell/branches/main), [gitui](https://api.github.com/repos/gitui-org/gitui/branches/master), and [lazygit](https://api.github.com/repos/jesseduffield/lazygit/branches/master). Direct ruleset endpoints: [Ratatui](https://api.github.com/repos/ratatui/ratatui/rulesets), [Crossterm](https://api.github.com/repos/crossterm-rs/crossterm/rulesets), [Zellij](https://api.github.com/repos/zellij-org/zellij/rulesets), [Helix](https://api.github.com/repos/helix-editor/helix/rulesets), [Alacritty](https://api.github.com/repos/alacritty/alacritty/rulesets), [WezTerm](https://api.github.com/repos/wezterm/wezterm/rulesets), [ripgrep](https://api.github.com/repos/BurntSushi/ripgrep/rulesets), [fd](https://api.github.com/repos/sharkdp/fd/rulesets), [bat](https://api.github.com/repos/sharkdp/bat/rulesets), [Nushell](https://api.github.com/repos/nushell/nushell/rulesets), [gitui](https://api.github.com/repos/gitui-org/gitui/rulesets), and [lazygit](https://api.github.com/repos/jesseduffield/lazygit/rulesets). Ratatui's returned ruleset detail is [ruleset 17633489](https://api.github.com/repos/ratatui/ratatui/rulesets/17633489).
[^62]: Tobias Bieniek on behalf of the crates.io team, “crates.io: development update,” 2025-07-11, trusted publishing section, [Rust Blog](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/).
[^63]: The Rust Project, “Publishing on crates.io,” current Cargo guide, [doc.rust-lang.org](https://doc.rust-lang.org/cargo/reference/publishing.html).
[^64]: GitHub REST API, “Get immutable releases settings,” owner-authenticated read-only response for Proqi on 2026-09-09 using API version `2026-03-10`, `enabled: true`, `enforced_by_owner: false`, [REST documentation](https://docs.github.com/en/rest/repos/repos?apiVersion=2026-03-10#get-immutable-releases-settings), [repository endpoint](https://api.github.com/repos/oborchers/proqi/immutable-releases).
