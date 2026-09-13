# Plan 064 — Depot runners and Intel legacy support

Status: ROLLOUT IN PROGRESS — repository transfer complete; canonical URL
cutover included on PR #140. Routine and release Depot variables remain disabled.
No release action is authorized here.
Base: `f032254a` (PR #140 follow-up head).
Depends on: 042 (cache correctness), 062 (cheap-check gates and cancellation).

## Decision

Keep GitHub Actions as the workflow, checks, artifact, signing, GitHub Release,
and updater control plane. Depot CI itself is not a viable replacement for this
native pipeline: its sandboxes are Linux-only and its compatibility matrix does
not support `release` events, environments, or fork PR execution. Use Depot's
GitHub Actions runners only for trusted Apple Silicon macOS and Windows x64 jobs.
Keep Ubuntu control-plane jobs on GitHub's standard public-repository runners.

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

Remaining external rollout:

1. Keep the separately hosted website repository and its Vercel project outside
   desktop-repository provider changes. Do not change organization-wide Vercel
   access because that could affect other repositories.
2. Confirm the Depot offer/plan and configure spend caps or alerts. Current
   Depot docs place macOS runners on Startup/Business, and the published PostHog
   offer charges the card after credits expire.
3. Run a separately approved non-release CI pilot with `use_depot=true`; only
   after it succeeds should `DEPOT_RUNNERS_ENABLED=true` route trusted routine
   native jobs to Depot.
4. Keep `DEPOT_RELEASE_RUNNERS_ENABLED` unset until an explicit signed dry-run
   release proves signing, notarization, artifacts, updater signatures and
   exact filenames.

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
- A manual CI dispatch may set `use_depot=true` for a controlled runner pilot.
  Once `DEPOT_RUNNERS_ENABLED=true`, trusted same-repository/manual/main ARM Mac
  and Windows jobs use `depot-macos-14` and `depot-windows-2022`; fork PRs stay
  on GitHub-hosted runners so untrusted contributors cannot consume Depot credit.
- Automatic CI includes Apple Silicon and Windows x64 only. A manual CI dispatch
  exposes `include_intel`; true adds the existing `macos-15-intel` lane.
- Store MSIX is manual-only and checks out a required immutable commit SHA. It
  may use Depot Windows after the CI pilot; it is not an updater/release asset.
- Release remains manual and keeps Intel on GitHub. A distinct
  `DEPOT_RELEASE_RUNNERS_ENABLED=true` may move only ARM Mac and Windows build
  jobs after a successful release dry run. Prepare, publish, release assembly,
  beta-channel publication, artifact names and updater manifests stay unchanged.
- Native jobs have explicit timeout ceilings so a stuck runner cannot consume
  unbounded credit. Draft conversion cancels the previous PR run.

## Support contract

Apple Silicon macOS is fully supported and remains the primary Mac download.
Intel macOS is a legacy download: Whisper remains available; Apple Silicon-only
Parakeet is unavailable; critical compatibility fixes are best-effort. Continue
producing the Intel artifact in releases until a separately announced EOL. A
future EOL needs a final Intel build and a static legacy updater route; it must
not simply delete `darwin-x86_64` from the shared updater manifest.

## Verification

- Workflow helper tests cover default ARM-only/manual Intel matrices and
  frontend-only/native fail-closed classification.
- Pinned actionlint 1.7.7 validates every GitHub workflow, including missing
  variable fallbacks and runner expressions.
- `pnpm build` passes and now runs in the cheap Ubuntu prerequisite, so
  frontend-only changes cannot skip production bundle validation.
- Independent workflow review is clear after adding that production build.
- After the provider and spending checks, repeat the non-release ARM/Windows
  pilot and compare elapsed time, queue time, cache hit rate and billed usage.
  Only then consider the separate release runner flag.

Local result: 19 workflow helper tests passed; Node syntax, actionlint,
production frontend build and `git diff --check` passed. The canonical URL
patch passes shell/JSON/Rust formatting checks and the focused updater-channel
test. PowerShell syntax was not executed because `pwsh` is unavailable locally.

The first manual Depot pilot was started before a separate point-of-spend
confirmation and canceled. Workflow/frontend prerequisites passed; its ARM and
Windows native jobs started and were canceled, so no native result is accepted.
Some metered usage may have occurred. Routine/release Depot variables remain
unset; Store and release workflows did not run.

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

No Depot account mutation, plan purchase, credit redemption, branch-protection
mutation, release, beta publication, Intel artifact removal, legacy updater
removal, or full Depot CI migration.
