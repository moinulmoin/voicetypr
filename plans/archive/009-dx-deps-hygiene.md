# Plan 009: DX & dependency hygiene — CI-parity quality gate, unused frontend plugin, shadcn placement, dotenv gating

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- scripts/quality-gate-check.sh package.json src/test/setup.ts src-tauri/src/lib.rs .github/workflows/ci.yml`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx / deps
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

Four small, verified hygiene gaps:

1. **`pnpm quality-gate` is not CI parity.** CI additionally runs the
   formatting-sidecar TypeScript check and `cargo clippy -- -D warnings`
   (`.github/workflows/ci.yml:34-35` and `:88-89`); the local gate doesn't,
   so "gate green" can still fail CI.
2. **`@tauri-apps/plugin-global-shortcut` is an unused frontend dependency.**
   The only reference under `src/` is a vitest mock (`src/test/setup.ts:199`);
   production code invokes backend commands instead
   (`src/components/sections/GeneralSettings.tsx:261`,
   `src/components/onboarding/OnboardingDesktop.tsx:477`). (The RUST plugin
   `tauri-plugin-global-shortcut` is heavily used — NOT touched.)
3. **`shadcn` (a codegen CLI) sits in production `dependencies`** and is the
   source of all 7 production-profile `pnpm audit` findings (via its
   `fast-glob`/`postcss`/`@modelcontextprotocol/sdk>hono` chains). It belongs
   in `devDependencies`; it is never imported by app code.
4. **`dotenv` loads unconditionally at startup** (`src-tauri/src/lib.rs:161`),
   so a stray `.env` in the working directory of a *production* app can
   override runtime config (e.g. `VITE_BUG_REPORT_ENDPOINT` per
   `.env.example`). Gate it to debug builds.

## Current state

`scripts/quality-gate-check.sh` (entire file, 25 lines):

```bash
echo -e "${YELLOW}[1/4] Type checking...${NC}"
pnpm typecheck
echo -e "${YELLOW}[2/4] Linting...${NC}"
pnpm lint
echo -e "${YELLOW}[3/4] Frontend tests...${NC}"
pnpm test run
echo -e "${YELLOW}[4/4] Backend tests...${NC}"
pnpm test:backend
```

`.github/workflows/ci.yml` gates (frontend job + macOS job):

```yaml
      - name: Formatting sidecar TypeScript check          # :34-35
        run: pnpm --filter @voicetypr/formatting-engine-sidecar exec tsc --noEmit
      ...
      - name: Clippy                                        # :88-89
        run: cd src-tauri && cargo clippy -- -D warnings
```

(Windows CI runs `cargo test --no-run` only — compile-only; that is a known,
accepted limitation to DOCUMENT, not fix, in this plan.)

`package.json:41`: `"@tauri-apps/plugin-global-shortcut": "^2.3.0"` under
`dependencies`. `package.json:57`: `"shadcn": "^4.7.0"` under `dependencies`.

`src/test/setup.ts:198-204` (approx):

```ts
// Mock global shortcut plugin
vi.mock('@tauri-apps/plugin-global-shortcut', () => ({
  GlobalShortcutExt: vi.fn(),
  ShortcutState: { Pressed: 'pressed', ... },
}))
```

`src-tauri/src/lib.rs:159-170`:

```rust
    // Load .env file if it exists (for development)
    log_start("ENV_FILE_LOAD");
    match dotenv::dotenv() {
        Ok(path) => {
            log_file_operation("LOAD", &format!("{:?}", path), true, None, None);
            println!("Loaded .env file from: {:?}", path);
        }
        Err(e) => { ... }
    }
```

## Commands you will need

| Purpose            | Command                                         | Expected on success |
|--------------------|--------------------------------------------------|---------------------|
| Install            | `pnpm install`                                   | exit 0, lockfile updated |
| Typecheck          | `pnpm typecheck`                                 | exit 0              |
| Lint               | `pnpm lint`                                      | exit 0              |
| Frontend tests     | `pnpm exec vitest run`                           | all pass            |
| Backend compile    | `cd src-tauri && cargo check`                    | exit 0              |
| Full gate          | `pnpm quality-gate`                              | all steps pass      |
| Prod audit         | `pnpm audit --prod`                              | 0 vulnerabilities   |

## Scope

**In scope**:
- `scripts/quality-gate-check.sh`
- `package.json` (+ `pnpm-lock.yaml` via install)
- `src/test/setup.ts`
- `src-tauri/src/lib.rs` (dotenv block only)
- `AGENTS.md` — one short bullet documenting Windows CI's compile-only Rust
  tests (Gotchas section)

**Out of scope**:
- `src-tauri/Cargo.toml` — `tauri-plugin-global-shortcut` (Rust) stays;
  `dotenv` dependency stays (still used in debug).
- `src-tauri/capabilities/*.json` `global-shortcut:*` permissions — they
  authorize the RUST plugin's commands; removing them breaks hotkeys.
- The whisper model catalog `sha256`-field misnaming (deferred; see
  plans/README.md rejected/deferred section).
- `.github/workflows/ci.yml` itself.
- Upgrading any dependency version.

## Git workflow

- Branch: `advisor/009-dx-deps-hygiene`
- Commit message: `chore: align quality gate with ci and prune dep hygiene`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Quality gate parity

Edit `scripts/quality-gate-check.sh`: renumber to `[1/6]`…`[6/6]` and add,
mirroring CI exactly:

```bash
echo -e "${YELLOW}[3/6] Formatting sidecar TypeScript check...${NC}"
pnpm --filter @voicetypr/formatting-engine-sidecar exec tsc --noEmit
...
echo -e "${YELLOW}[6/6] Clippy...${NC}"
(cd src-tauri && cargo clippy -- -D warnings)
```

Order: typecheck, lint, sidecar tsc, frontend tests, backend tests, clippy.

**Verify**: `pnpm quality-gate` → all six steps run and pass.

### Step 2: Remove the unused frontend plugin

1. `grep -rn "@tauri-apps/plugin-global-shortcut" src/` → confirm the only
   hit is the mock in `src/test/setup.ts`.
2. Delete that `vi.mock(...)` block.
3. Remove `"@tauri-apps/plugin-global-shortcut"` from `package.json`
   dependencies.
4. `pnpm install`.

**Verify**: `pnpm exec vitest run` → all pass;
`grep -rn "plugin-global-shortcut" src/ package.json` → no matches.

### Step 3: Move `shadcn` to devDependencies

Move the `"shadcn": "^4.7.0"` entry from `dependencies` to
`devDependencies`; `pnpm install`.

**Verify**: `pnpm audit --prod` → `No known vulnerabilities found` (or 0
listed); `pnpm typecheck` and `pnpm build` → exit 0 (proves nothing imports
it at build time from the prod graph).

### Step 4: Gate dotenv to debug builds

Wrap the entire `.env` block in `lib.rs` (`log_start("ENV_FILE_LOAD")`
through the closing brace of the `match`) in:

```rust
    #[cfg(debug_assertions)]
    {
        // Load .env file if it exists (development builds only)
        ...existing block...
    }
```

Remove the `println!` lines while you're in there (logging already covers
it) — they are part of the same block.

**Verify**: `cd src-tauri && cargo check` → exit 0. Confirm no other
`dotenv::` call exists: `grep -rn "dotenv" src-tauri/src/` → only the gated
block.

### Step 5: Document the Windows CI limitation

In `AGENTS.md` under "Gotchas", add one bullet:
`9. **Windows CI is compile-only for Rust tests**: \`cargo test --no-run\` — Windows runtime behavior (hotkeys, Vulkan sidecar) needs manual smoke on a real machine.`

**Verify**: bullet present; `git diff AGENTS.md` shows only the addition.

### Step 6: Full gate

**Verify**: `pnpm quality-gate` → all pass. `cd src-tauri && cargo fmt --check`
→ exit 0.

## Test plan

No new tests — every change is gated by an existing command above. The
frontend suite run in Step 2 is the regression net for the mock removal.

## Done criteria

ALL must hold:

- [ ] `pnpm quality-gate` runs 6 steps including sidecar tsc + clippy
- [ ] `grep -rn "plugin-global-shortcut" src/ package.json` → no matches
- [ ] `shadcn` under `devDependencies`; `pnpm audit --prod` → 0 vulns
- [ ] `dotenv::dotenv()` only under `#[cfg(debug_assertions)]`
- [ ] AGENTS.md gotcha added
- [ ] `pnpm exec vitest run` all pass; `cargo check` exit 0
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- Step 2's grep finds a production import of the frontend plugin (the audit
  missed one) — report instead of removing.
- `pnpm build` fails after the shadcn move (something imports it at build
  time).
- Any test asserts on the dotenv `println!`/log lines.
- `pnpm quality-gate` clippy step fails on PRE-EXISTING warnings unrelated to
  this plan — report the list; do not fix unrelated clippy findings here.

## Maintenance notes

- If clippy-in-gate proves too slow for everyday use, split a
  `quality-gate:fast` script — but keep the default gate CI-parity.
- The deferred checksum-field rename (`sha256` field holding SHA1 values in
  `src-tauri/src/whisper/manager.rs:96-99,425-432`) is recorded in
  plans/README.md; pick it up when the model catalog next changes.
