# Plan 064 — Depot runners and Intel legacy support

Status: CODE COMPLETE / EXTERNAL SETUP REQUIRED — local branch
`feat/064-depot-runners`; no account, billing, repository ownership, branch
protection, or release action is authorized here.
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

## Eligibility and rollout blockers

The repository is currently owned by the personal GitHub account `moinulmoin`.
Depot's managed GitHub Actions runners require an organization-owned repository.
Missing repository variables therefore MUST preserve GitHub-hosted runners.
Enabling Depot requires an explicit, external rollout after this code lands:

1. Decide/create the GitHub organization and transfer the repository separately.
   Audit hard-coded repository URLs, GitHub App installations, branch settings,
   signing secrets, release permissions, updater redirects, and the website
   before transfer. Repository transfer is not part of this plan.
2. Redeem/activate the Depot offer and choose a plan. Current Depot docs place
   macOS runners on Startup/Business. The published PostHog offer covers plan and
   usage for one year, then charges the card after credits are exhausted.
3. Install the Depot GitHub App for the organization/repository and configure a
   spend cap/alerts. No workflow may silently activate billing.
4. Run a manual CI pilot with `use_depot=true`; only after it succeeds should
   `DEPOT_RUNNERS_ENABLED=true` make trusted routine native jobs use Depot.
5. Keep `DEPOT_RELEASE_RUNNERS_ENABLED` unset until an explicit dry-run release
   proves signing, notarization, artifacts, updater signatures, and exact names.

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
- After external Depot setup, pilot a non-release ARM/Windows run and compare
  elapsed time, queue time, cache hit rate and billed usage. Only then consider
  the separate release runner flag.

Local result: 19 workflow helper tests passed; Node syntax, actionlint,
production frontend build and `git diff --check` passed. No Depot runner,
Store package, native release, account or billing operation was executed.

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

No Depot account mutation, plan purchase, credit redemption, repository transfer,
secret migration, branch-protection mutation, release, beta publication, Intel
artifact removal, updater endpoint change, or full Depot CI migration.
