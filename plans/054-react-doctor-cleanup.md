# Plan 054 — react-doctor full-court cleanup

Baseline after 052/053: 62/100 ("Needs work"), 93 findings.
Goal: clear every mechanical finding; split giant components only where the
score demands it. Behavior-preserving; tests after each wave.

## Waves (parallel, file-disjoint)

1. **HookBugHygiene** — impure state updater (critical), loading flags in
   finally, exhaustive-deps, set-state-after-await, lazy ref init,
   pure-fn hoists, only-export moves, RecentRecordings a11y.
2. **PerfContextStability** — context value memoization (License,
   Settings, toggle-group), noopener, fetch status check, Set/Map lookups,
   constant-JSX hoist, ui-primitive variant exports to sibling modules.
3. **SectionsPerfA11y** — stable list keys, transition-all, accessible
   labels, combined iterations, Promise.all for independent awaits.

## After waves

- Re-run react-doctor; decide how far to push `no-giant-component` ×14
  (each split is a plan-051-style refactor — only do what the score needs).
- Full `pnpm vitest run`, `pnpm typecheck`, `pnpm lint`, `pnpm build`.
- Live CUA smoke of the touched surfaces.

## Acceptance

Score materially raised from 62; zero regressions in the 663-test suite;
finding-by-finding accounting (fixed / skipped-because-false-positive).
