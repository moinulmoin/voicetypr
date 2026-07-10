# HANDOFF — AI Polish Overhaul (Plan 030)

> For any agent taking over this work. Read this fully before touching anything.
> Written 2026-07-10. Branch state at handoff: `feat/ai-polish-personalization` @ `b82f869a`, 35 commits ahead of `main`, all pushed, all gates green.

## 1. What this is

VoiceTypr (Tauri v2 desktop app: Rust backend `src-tauri/`, React/TS frontend `src/`) does offline voice transcription. "Polish" is the optional AI post-processing step that cleans the raw transcript before it's typed into the active app.

**Mission:** make Polish as good as Wispr Flow, with a two-persona split as the architectural spine:
- **Regular users**: amazing zero-config defaults, ONE switch. They never see complexity.
- **Advanced users**: deep configurability, tucked under Advanced.

Every design decision on this branch was made through that lens. Keep it.

## 2. Where the work lives

- **Worktree:** `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-ai-polish-personalization`
- **Branch:** `feat/ai-polish-personalization` (pushed to origin)
- **Plan docs:** `plans/030-ai-polish-personalization.md` (master), `plans/030-phase4b-app-context-spec.md`, `plans/030-phase4c-agent-cli-spec.md` (has EMPIRICAL FINDINGS — read before touching agent-CLI code)

## 3. Workflow rules (founder-mandated, do not deviate)

1. **Research before implementing.** Founder rule: "do good codebase research before you fix/implement anything to avoid regressions." If unsure → web research first, then ask the founder. It's 2026 — search accordingly.
2. **Tauri-first, not Rust-first.** Check for an existing Tauri plugin/solution before hand-rolling Rust.
3. **Role split for delegated work:** GLM 5.2 IMPLEMENTS, Codex REVIEWS, the orchestrating agent gates + reviews (dual review). GLM launch template:
   ```
   omp -p --auto-approve --no-title --no-skills --no-rules --no-extensions \
       --thinking xhigh --tools read,edit,grep,bash --max-time <N> \
       --cwd <worktree> --model zai/glm-5.2 "$(cat taskfile)"
   ```
   Minimal tools only, xhigh thinking. GLM sometimes hits `--max-time` after actually finishing — verify with `cargo check` + gates before assuming failure.
4. **Marketing rule:** NEVER say "Whisper" in user-facing copy (say local/offline AI). Founder approves ALL user-facing copy before ship.
5. **Don't over-checkpoint.** Build + gate autonomously; surface only real decisions/blockers/completions. Founder: "keep moving."
6. **Never push without explicit founder instruction** (this branch has standing push permission; a new branch does not).

## 4. Gates (all must be green before commit)

```bash
pnpm typecheck && pnpm lint && pnpm test        # frontend (589 tests at handoff)
cd src-tauri && cargo test && cargo clippy      # backend (1247 tests at handoff)
```
Known env quirks:
- macOS 26 (Tahoe): local whisper Metal encode fails in the harness — CI and shipped builds are fine; don't chase it.
- Transient cargo "failed to create dependency graph" → re-run with `CARGO_INCREMENTAL=0`.

## 5. What was built (35 commits, in narrative order)

### Phase 1 — polish quality core
- Pinned sampling params + bounded max output tokens for every polish call (`src-tauri/src/ai/`).
- Hardened polish prompt; translation is explicit opt-in only (`ai/prompts.rs`).
- **Output validation + repair before insertion** — garbage model output (echoes, wrappers, "Here's your text:", empty) is repaired or the RAW TRANSCRIPT is used. Raw transcript is the sacred fallback on every failure path. Never weaken this.
- Per-stage latency instrumentation.

### Phase 2 — state collapse (refactors, no behavior change)
- Split `writing.rs` god-file into `src-tauri/src/writing/` modules.
- **Deleted `WritingMode` entirely** — presets are the single source of truth, one resolver.
- Centralized recording-config cache invalidation.
- **Shared Rust/TS golden fixture** `tests/fixtures/preset-parity.json` — both sides assert preset definitions byte-for-byte. If you change a preset, update the fixture and BOTH test suites will tell you.

### Phase 3 — two-persona UX
- Three formatting screens merged into ONE Polish screen (`src/components/sections/EnhancementsSection.tsx` — the big file).
- Guided BYOK provider setup (not a paywall look); connected status.
- Onboarding simplified; recording pill says "Polishing…".
- Tray shows + toggles Polish; per-mode formatting shortcuts retired (only Toggle Polish remains).
- Lockstep fixes: tray/shortcut can never enable an inert Polish (preset normalizes to Clean if needed).

### Phase 4A — OpenRouter
- First-class provider via the `openai_compatible` runtime. Curated model catalog.
- Catalog is data-driven: `catalog.generated.json` + `overlay.json` + `generate.py`; `CatalogProvider.runtime` ∈ {`genai_adapter`, `openai_compatible`, `agent_cli`}; dispatch in `ai/executor.rs::execute_once`.

### Phase 4B — app-context awareness
- `src-tauri/src/writing/app_category.rs`: frontmost app name → whole-word match against curated lists → 9 categories (Chat/Email/Docs/Code/Terminal/Social/Notes/Browser/Other) → one behavioral sentence injected into the prompt via `category_prompt_hint`.
- Whole-word matching is load-bearing ("1Password" ≠ Docs, "Barcode Scanner" ≠ Code) — tested.
- **Unknown/Browser → NO hint** (safe neutral). Local-only; app identity never leaves the machine.

### Phase 4C — agent-CLI providers (the headline feature)
Claude Code, pi, oh-my-pi (omp) as NO-API-KEY polish providers: spawn the user's already-installed, already-logged-in CLI headless one-shot; their subscription pays; we configure nothing.

Key file: `src-tauri/src/ai/agent_cli.rs`. Architecture:
- `AgentCliSpec` table: `InputMode` (Stdin for claude/pi | PositionalArg for omp), `OutputParser` (ClaudeJson | PiJsonl), `AuthMode` (RealAuthStatus for claude | Optimistic for pi/omp).
- `cold_spawn_and_collect` uses `tokio::process::Command` (NOT tauri-plugin-shell — trusted Rust code isn't subject to shell ACL scope). Empty temp cwd. `kill_on_drop(true)`, 9s deadline → raw-transcript fallback.
- **Login-shell PATH resolver** (cached `OnceLock`, `$SHELL -ilc` env dump with timeout+fallback) — Finder-launched macOS apps get a stripped PATH; without this, no CLI is ever found.
- Dictated text passed via stdin (claude/pi) or discrete `Command::arg` (omp) — injection-safe by construction.
- **Nonzero exit still parses stdout** — a not-logged-in CLI exits nonzero WITH its own useful message; we surface that message verbatim to the user (founder's UX idea: the CLI teaches the user the fix, e.g. "Not logged in · run /login"). Toast path: `AiProviderError::AgentCli(String)` → `emit_enhancing_failed` payload `{category:"cli_error", message}` → `AppContainer.tsx` listener (gated to cli_error so cloud failures stay silent).

**EMPIRICAL FINDINGS (hard-won, do not re-litigate without re-testing on a real logged-in machine):**
- Claude Code command (verified authed, ~2.7s): `claude -p --setting-sources "" --tools "" --strict-mcp-config --no-chrome --model haiku --system-prompt <PROMPT> --output-format json`, stdin input, empty cwd.
- `--bare` BREAKS subscription auth (returns "Not logged in"). `--setting-sources ""` is the correct isolation flag (skips CLAUDE.md/plugins/hooks, keeps Keychain creds).
- pi/omp `--mode json` = JSONL event stream; polished text = last assistant `message.content[].text`. pi reads stdin (~6.7s); omp does NOT read stdin in json mode → positional arg (~3.7s).
- Warm-persistent sessions were DROPPED: cold spawn is model-call-dominated; warm saves only ~0.5s. Not worth the complexity.
- MSIX gating was DROPPED: full-trust MSIX (which VoiceTypr uses) CAN spawn external CLIs — VoiceTypr's own MSIX spawns ffmpeg/whisper sidecars. Ships on all channels.
- Model default = `haiku` (cheapest).
- **Lesson: external-CLI integrations MUST be smoke-tested on a real logged-in setup.** Static review + compile + unit tests missed `--bare` breaking auth. If you change any spawn flag, re-verify live.

### Frontend for 4C
- `src/types/providers.ts`: `AgentCliProbe`, `installHint`, UI metadata for the three CLIs.
- `EnhancementsSection.tsx`: `AGENT_CLI_PROVIDER_IDS` / `isAgentCliProvider`, `agentCliStatus` probe state, `handleRefreshAgentCli`, 3-state badge (not installed / installed-not-signed-in / ready). Guided setup must NEVER open the API-key modal for a CLI provider (regression-tested).
- Backend exemptions in `commands/ai.rs`: agent_cli providers need no key and no model list (`list_provider_models` → Ok(empty); `has_ai_model_and_key` → true; per-runtime timeout 9s CLI / 30s HTTP; `probe_agent_cli` command).

## 6. Bugs found by review (all fixed, all regression-tested)

The dual-review loop (GLM implements → Codex reviews → orchestrator gates) caught real bugs; keep the loop:
1. Nonzero CLI exit early-returned BadResponse and swallowed the CLI's own error message → parse stdout regardless of exit.
2. `list_provider_models` errored on model-less CLI providers → Ok(empty) + frontend skips fetch.
3. Guided setup opened the API-key modal for a not-ready CLI provider → `isAgentCliProvider` guard + toast guidance (`b82f869`).

## 7. What remains (in priority order)

1. **Founder smoke test** — QA list (founder has it): baseline polish, Claude Code dictation, CLI-error UX (logout → raw transcript + CLI's toast → Refresh), app-context in Slack vs Mail vs terminal, Polish OFF sanity, tray toggle, settings migration, pi/omp once each. Items 1–5 gate the merge.
2. **Copy pass** — user-facing strings are functional placeholders: `installHint`s in `src/types/providers.ts`, sign-in toasts in `EnhancementsSection.tsx` (~line 687), "Which AI should I pick?" dialog (~line 966). FOUNDER MUST APPROVE all copy (and: never the word "Whisper").
3. **Merge to main** — after 1+2. Founder decides.
4. Optional post-merge: haiku/sonnet/opus model picker under Advanced (claude supports `--model`); OpenRouter $/M cost display in the picker; browser tab-URL context (opt-in, Apple Events per browser, domain→category mapping; needs permission-prompt onboarding + privacy framing — deserves its own small plan).

## 8. Founder context (how to work with him)

- Voice-dictates messages — expect loose phrasing; extract intent, confirm only real ambiguities.
- Wants momentum: don't pause at green checkpoints; do pause for product decisions and anything user-facing (copy!).
- Will challenge unverified claims ("are you sure?") — bring evidence, not vibes. Two claims died on this branch that way (MSIX gating, warm sessions).
- Default English for transcription everywhere; auto-detect language intentionally NOT offered.
- Environment note: Codex CLI stop-review gate failed repeatedly on 2026-07-10 → root cause was an outdated Codex CLI (0.142.5) + stale broker daemons holding the old binary; fixed by `codex update` (→0.144.1) and killing brokers. If the gate flaps again with empty output, check `codex exec` directly and broker process age.
