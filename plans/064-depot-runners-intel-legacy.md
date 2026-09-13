# Plan 064 — Depot runners and Intel legacy support

Status: PORTABLE SECURE PILOT COMPLETE — repository transfer and canonical URL
cutover landed on PR #140, and current-head pilot `34785226804` proved the
fork-safe admission design on Depot after the GitHub-hosted fallback passed.
Routine and release Depot variables remain unset pending explicit
routine-enablement approval, a signed no-publish release dry run, and release
runner allowlisting. No release action is authorized here.
Base: `e9a022fc` (portable pilot head `e9a022fc94a4978fc41218b177af368389bc9cb7`).
Depends on: 042 (cache correctness), 062 (cheap-check gates and cancellation).

## Decision

Keep GitHub Actions as the workflow, checks, artifact, signing, GitHub Release,
and updater control plane. Depot CI itself is not a viable replacement for this
native pipeline: its sandboxes are Linux-only and its compatibility matrix does
not support `release` events, environments, or fork PR execution. Use Depot's
GitHub Actions runners only for trusted Apple Silicon macOS and Windows x64 jobs.
Keep Ubuntu control-plane jobs on GitHub's standard public-repository runners.
Depot admission is enforced by a reusable workflow pinned to an immutable
commit SHA plus the runner group's Selected workflows allowlist — never by
caller YAML that a pull request can rewrite.

Intel macOS becomes legacy/best-effort: no automatic PR lane and no required
merge check. It remains a separate x86_64 artifact in the explicit release
workflow and may be requested by a maintainer in a manual full CI dispatch.
Do not remove `darwin-x86_64` from `latest.json` or strand installed Intel users.

## Rollout state and blockers

The public repository now lives at `ideaplexa/voicetypr`. GitHub confirms the
old repository path redirects, PR #140 and releases moved, and all nine Actions
secrets remain associated. The local `origin` uses the canonical organization.
Never recreate `moinulmoin/voicetypr`, which would permanently remove redirects.

Transfer audit:

- The authenticated user is an active `ideaplexa` admin. The destination had no
  conflicting repository. The repository has no GitHub Pages, repository
  webhooks, deploy keys, rulesets, or environments.
- Vercel, Coolify and Depot GitHub Apps have organization-wide repository
  access. Permission is not proof of an active provider project or deployment.
  GitHub reports zero repository deployments. Vercel/Netlify/Railway/Cloudflare
  check suites on the last pre-transfer head were queued with zero check runs.
- Coolify is the intended deployment integration. Cloudflare remains the DNS
  provider for `voicetypr.com`; the repository transfer does not modify DNS.
- Historical changelog links may rely on redirects. Runtime updater endpoints,
  Help links, badges, clone URLs, and manual release scripts use the canonical
  new owner on PR #140 and must land before the next release.
- Depot's Default runner group has public repository runner access ON, which
  is required for this public repository. Public access alone is not the
  isolation mechanism: the group's Selected workflows allowlist contains only
  the pinned `native-ci.yml`, every Depot-eligible job is defined inside that
  trusted callee with fixed labels and timeouts, and fork PRs can only select
  literal GitHub-hosted runners with a read-only token and no retained
  checkout credentials.

Provider controls confirmed with Depot: the Startup plan is active, and
monthly caps are Container 5,000, GitHub Actions 20,000 billable minutes,
Depot CI 20,000 minutes, and 25 GB each for the GitHub Actions cache and
Depot Cache.

Remaining external rollout:

1. Routine enablement: `DEPOT_RUNNERS_ENABLED` stays unset until a maintainer
   explicitly approves recurring Depot use for trusted routine native jobs.
2. Release dry run: run a separate signed, notarized, no-publish release dry
   run — `dry_run` builds signed and notarized artifacts while the version
   commit, tag, publish, and GitHub release jobs stay off — before any
   `DEPOT_RELEASE_RUNNERS_ENABLED` is considered.
3. Release allowlisting: `store-msix.yml` and `release.yml` are not in the
   Default runner group's Selected workflows allowlist. Keeping Store
   `use_depot=false` and `DEPOT_RELEASE_RUNNERS_ENABLED` unset is what
   selects GitHub-hosted runners — there is no automatic fallback. Enable in
   order: first extend the allowlist with the intended immutable workflow
   refs, then select Depot. If Depot is selected while the workflow is
   disallowed, the job remains unassigned/denied.

## Workflow contract

- Keep draft PR iteration cheap: workflow/frontend checks run, but native jobs
  wait until the PR is ready. Local commits are free; batch reviewed commits
  into deliberate pushes rather than treating every local commit as a remote
  validation boundary. After a ready PR changes, cancellation ensures only the
  newest pushed revision continues.
- Classify frontend-only changes separately from native/build inputs. Frontend
  source still receives type/lint/test coverage without provisioning macOS and
  Windows. Unknown build inputs fail closed to native validation.
- Every PR keeps GitHub-hosted workflow/frontend prerequisites. Superseded runs
  cancel. Native jobs start only after those prerequisites pass, only for native
  or unknown build inputs, and not while the PR is draft.
- CI calls the reusable `native-ci.yml` pinned to immutable SHA
  `00e8006fb7efd9f7c6a8ffb2bcc9c30c8a298c52`; only that exact workflow is in
  the Default runner group's Selected workflows allowlist. Every
  Depot-eligible job is defined directly in the trusted callee with fixed
  labels and admission logic; callers cannot inject a runner label, matrix,
  timeout, trust boolean, or checkout ref. A manual dispatch may set
  `use_depot=true` for a controlled pilot; that pilot has passed.
- Depot admission is exactly: same-repository pull request, push to
  `refs/heads/main`, or workflow dispatch, combined with
  `DEPOT_RUNNERS_ENABLED=true` or the manual `use_depot` pilot input. Fork
  pull_request events and every other caller resolve to literal
  GitHub-hosted runners, so untrusted contributors cannot consume Depot
  credit. Once enabled, trusted jobs use `depot-macos-14` and
  `depot-windows-2022-16`; the 16-core, 64 GB Windows tier is intentional
  because the unsuffixed Depot label is only two-core/8 GB and was slower
  than GitHub's free public runner.
- Automatic CI includes Apple Silicon and Windows x64 only. A manual CI
  dispatch exposes `include_intel`; true adds the existing `macos-15-intel`
  lane. Intel is a direct manual-only GitHub job; it is never Depot-eligible.
- Store MSIX is manual-only and accepts only exact lowercase 40-character
  SHAs verified as ancestors of `origin/main` with full history before
  packaging. Its Windows job selects GitHub-hosted runners only while
  `use_depot` stays false; selecting the Depot label before the workflow is
  allowlisted leaves the job unassigned/denied, so allowlist extension must
  precede any Store Depot use. Store MSIX is not an updater/release asset.
- Release remains manual and keeps Intel on GitHub. `dry_run` builds signed
  and notarized artifacts while the version commit, tag, publish, and GitHub
  release jobs stay off. A distinct `DEPOT_RELEASE_RUNNERS_ENABLED=true` may
  move only the ARM Mac and Windows build jobs, and only after that
  no-publish dry run passes. Prepare, publish, release assembly,
  beta-channel publication, artifact names and updater manifests stay
  unchanged.
- Native jobs have explicit timeout ceilings (macOS 90, Windows 120 minutes)
  fixed in the trusted callee, so a stuck runner cannot consume unbounded
  credit. Draft conversion cancels the previous PR run.

## Support contract

Apple Silicon macOS is fully supported and remains the primary Mac download.
Intel macOS is a legacy download: Whisper remains available; Apple Silicon-only
Parakeet is unavailable; critical compatibility fixes are best-effort. Continue
producing the Intel artifact in releases until a separately announced EOL. A
future EOL needs a final Intel build and a static legacy updater route; it must
not simply delete `darwin-x86_64` from the shared updater manifest.

## Verification

- Workflow helper tests cover documentation/frontend/native fail-closed
  classification and release version/update-manifest contracts.
- Pinned actionlint 1.7.7 validates every GitHub workflow, including missing
  variable fallbacks and runner expressions.
- `pnpm build` passes and now runs in the cheap Ubuntu prerequisite, so
  frontend-only changes cannot skip production bundle validation.
- Independent workflow review is clear after adding that production build; a
  later review found the admission bypass recorded below, now closed.
- Portable secure pilot `34785226804` at head
  `e9a022fc94a4978fc41218b177af368389bc9cb7` passed every scheduled job:
  workflow/front-end passed, the allowlisted Depot macOS lane passed in
  17m00s, and Windows-16 passed in 17m29s; Intel was skipped. Depot labels and
  runner names were observed on the expected Default group.

Local result: 18 workflow helper tests and pinned actionlint 1.7.7 pass after
the pilot corrections. An explicit compiler-rt link completed
`cargo test --no-run` locally, including both application test executables.
The production frontend build and canonical URL checks passed before the pilot.
PowerShell syntax was not executed locally because `pwsh` is unavailable;
the passing remote Windows pilots exercised those PowerShell steps.

The first manual Depot pilot was started before a separate point-of-spend
confirmation and canceled. Workflow/frontend prerequisites passed; its ARM and
Windows native jobs started and were canceled, so no native result is accepted.
Some metered usage may have occurred.

Authorized pilot `34768887709` initially queued because the runner group did not
allow public repositories. After that prerequisite was enabled, both Depot jobs
started. ARM macOS failed during Rust test linking: Depot selected Xcode 16.4,
whose whisper.cpp Metal objects reference `___isPlatformVersionAtLeast`, while
Rust's final link omitted `libclang_rt.osx.a`. CI and both macOS release lanes
now discover and link Xcode's compiler runtime. Windows used the unsuffixed
two-core/8 GB label, reached 100% CPU and 98% memory, and was canceled by the
user after 57m50s; its compile-only Rust tests passed, but the release build did
not complete. CI, Store and release routing were then switched to the
deliberate 16-core, 64 GB `depot-windows-2022-16` tier. Routine/release
Depot variables remain unset; Store and release workflows did not run.

Compatibility pilot `34773469966` passed with compiler-runtime linking and
the 16-core Windows label, proving the native lanes run on Depot. It did not
prove fork isolation: Oracle then found a matrix/YAML admission bypass — the
Depot admission expression (runner labels and matrix) lived in caller
workflow YAML that a fork pull request can rewrite, so a fork could steer
its own jobs onto the Default runner group. Compatibility success and
admission security are therefore tracked as separate proofs.

Corrective preflight run `34777780297` failed pinned actionlint with
ShellCheck SC2012 on the `ls`-based Xcode discovery before any native job;
no Depot native job ran. Xcode discovery is now find-based, and the secure
pilot proves that fix remotely.

Secure pilot `34778214659` at head
`2259e34e149c9ea43ded178596c8416a76a97e5b` ran through the pinned reusable
workflow under the Selected workflows allowlist and passed all scheduled
jobs: workflow/front-end passed, the allowlisted Depot macOS lane passed in
8m19s, Windows-16 passed in 17m11s, and Intel was skipped. Depot labels and
runner names were observed on the expected Default group. No Intel, Store,
release, signing, or deployment workload ran in this pilot, and
routine/release Depot variables remain unset.

The first automatic GitHub-hosted fallback on the pinned callee
(`34778145500`) exposed a cross-Xcode regression: Xcode 16.2 reported a
nonexistent compiler-runtime path even though that older toolchain neither
ships nor requires `libclang_rt.osx.a`. Compiler-runtime discovery is now
bundle-local and optional: Xcode 16.4 links the real archive, while older
toolchains continue without it. Current-head fallback run `34780572224`
passed all scheduled jobs at `e9a022fc`; GitHub-hosted macOS passed in 35m29s,
Windows passed in 22m35s, and Intel was skipped.

Final current-head Depot pilot `34785226804` then passed the same pinned
portable workflow: macOS passed in 17m00s, Windows-16 passed in 17m29s, and
Intel was skipped. No Intel, Store, release, signing, or deployment workload
ran; routine/release Depot variables remain unset.

Remote GitHub fallback proof on PR #140 heads `27afadd1` and `c2af36ab`:
Store and Intel stayed off and all five scheduled jobs passed. The first run
exposed `scripts/README.md` as an incorrectly native path, so Markdown anywhere
now uses the documentation fast path. PR #140 still ran ARM/Windows because its
cumulative diff contains application code. That is deliberate: classifying only
the most recent push would let a new documentation commit cancel an unfinished
native run for the preceding code commit. Savings therefore come from draft PRs,
batched reviewed pushes, cancellation of superseded runs, frontend-only PRs, and
manual Store/Intel lanes—not from weakening current-head validation.

## Non-goals

No branch-protection mutation, no `DEPOT_RUNNERS_ENABLED` or
`DEPOT_RELEASE_RUNNERS_ENABLED` flip, no release, beta publication, Intel
artifact removal, legacy updater removal, or full Depot CI migration.
