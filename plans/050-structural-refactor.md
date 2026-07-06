# Plan 050 — Structural refactor: god-file split + engine unification + settings hardening

> Reconciles TWO verified sources: the arch roadmap (`docs/review/2026-07-03-arch-roadmap.md`,
> waves 0/1/3a ALREADY SHIPPED via plans 046/047/048) and the fresh deep-review
> (`docs/review/2026-07-architecture-deep-review.md`, 3 parallel audits run 2026-07-06 against
> the post-046/047/048 + post-049 tree — its file:line refs are the current anchors).
> This plan is the remaining structural work, phased so every slice ships independently,
> behavior-preserving, gated. **No big-bang** (CLAUDE.local.md rule).
>
> ⚠️ NOT for the campaign PR #109. Own branch: `refactor/050-structure` off
> `integrate/perf-ux-2026-07` AFTER feat/049 merges. Worktree: voicetypr-integration
> (or a fresh one if 049 smoke drags on).
>
> ⚠️ Already shipped — do NOT re-do: #25/#24/#8/#16-discrete (046), #18 dead bytes command +
> #21 engine layer out of audio.rs (047), #16-full uploads-through-executor (048), #12/#15
> (slice 045), #28 ggml-metal exit assert (RunEvent::Exit cache clear, this campaign),
> D1/D3/D4-types safe-wins (fc13fa0). #14b line_json extraction stays DEFERRED (high risk,
> not a blocker — roadmap §7). #14a capability routing belongs to plan 043, not here.
>
> Guardrails from the deep-review (binding): NO uniform `trait Transcriber` — the executor's
> enum dispatch (executor.rs:176-326) is correct; unify INTO it. Don't merge the 2 sidecar
> IPC layers. Don't merge the 2 normalizers. ai.rs + writing.rs stay (cohesive).

## Execution model

Claude reasons/plans/gates, GLM 5.2 (omp) implements per-slice specs, Codex second-opinions
each phase's diff. Per-slice gate: `cargo build` + `clippy --workspace --all-targets -D warnings`
+ `cargo test` + `pnpm typecheck/lint/test` + windows-check dispatch. One commit per slice,
`pub use` shims keep external paths stable. Any slice that can't stay behavior-identical →
STOP and report, don't improvise.

## Phase A — Safe mechanical extractions (audio.rs 7182 → ~5k) + leaf dedup

Order matters (later slices import earlier ones). Each is structure-only.

1. **A1 · pill_toast → `commands/pill_toast.rs`** (S, low). audio.rs:62-63 + 470-663
   (`PillToastAction/Variant/EventPayload`, `next_toast_id`…`should_hide_pill`). Verified
   self-contained (only `AppHandle` + emit). 7 modules import these via audio.rs — keep shim.
2. **A2 · failure classification → `commands/transcription_failure.rs`** (M, low).
   audio.rs:983-1441 (TranscriptionFailure impl, classify_local_failure, ai_failure_*,
   is_non_speech_transcript…). Near-pure; tests move with it.
3. **A3 · retranscription status → `commands/retranscription.rs`** (S-M, low).
   audio.rs:700-982 (NormalizedTempFile, TranscriptionStatus, apply_retranscription_status,
   parse_history_key…).
4. **A4 · request/result builders → `commands/transcription_request_build.rs`** (M, low-med).
   audio.rs:1082-1545 (build_transcription_job, build_remote_*_request,
   compile_remote_request_context [re-export: remote.rs:1478 calls it], plan_desktop_writing_success…).
5. **A5 · history persistence → `commands/transcription_history.rs`** (M, low-med).
   The "transcriptions"-store functions (save_transcription* 5967-6183, get_/delete_/clear_/
   update_ 6185-7009). Sole touchers of that store. Needs A1/A6 imports.
6. **A6 · recording generation + in-flight tracking → `commands/recording_generation.rs`**
   (S, med — concurrency-load-bearing, mechanical move only). audio.rs:75, 297-470
   (RECORDING_GENERATION, IN_FLIGHT_TRANSCRIPTION_AUDIO, StopInFlightGuard…). stream_tap.rs +
   transcription/stream.rs depend via shim.
7. **A7 · Parakeet preview sink → `commands/parakeet_stream_sink.rs`** (S, low).
   audio.rs:77-315. NOTE: plan 043 renames/generalizes this — if 043 lands first, re-anchor.
8. **A8 · leaf dedup (deep-review F4+F5)** (S, low): one `seconds_to_duration_ms` (fold the
   f64 gpu_sidecar copy + engines.rs copy — engines.rs already has the finite guard from
   fc13fa0), one segment→TranscriptionSegment mapper, one `wav_duration_ms`
   (parakeet/messages.rs:219, gpu_sidecar.rs:784, executor.rs:486), one
   `transcription_budget()` + shared TIMEOUT consts (engines.rs:22, gpu_sidecar.rs:769,
   cloud_stt/common.rs:88, parakeet/messages.rs:110). Keep the sidecar's *layered* internal
   deadline — share only the formula.
9. **A9 · warm-up dedup (F6)** (S, low): one `warm_origin(client, url)`; keep the two client
   pools (intentional). Collapse the twin spawn blocks at audio.rs:3827/3843.

## Phase B — Engine unification (roadmap Wave-3 tail = deep-review F1/F2/F3)

10. **B1 · remote-send helper (F3/#19)** (S-M, low): one
    `send_remote_transcription(...)` for the 4 copies (audio.rs:5111 + 6477, remote.rs:1583,
    cli.rs:789). Pure extraction first; executor `Explicit{Remote}` route (Stage 5) only if
    trivially clean afterwards.
11. **B2 · single engine resolver (F2/#22)** (M, med): fold the fallback-model selection from
    stop_recording's inline resolver (audio.rs:4647-4849) into
    `resolve_engine_for_model` (engines.rs:242); delete the inline copy; kill the
    double-resolve-per-dictation. UI side-effects (model-fallback emit, toasts) stay at the
    call site via a returned decision enum — resolver stays pure.
12. **B3 · remote host through executor (F1/#17, Stage 4 `HostDefault`)** (L, med-high):
    remote/transcription.rs:276-482 re-implements the Whisper+Parakeet flow with its own
    TranscriberCache (2 caches, 2 mutex flavors). Route ServerContext::transcribe →
    `block_on(transcribe_with_app)` (it already block_ons Parakeet). Preserves: host
    serialization semantics, custom-vocab parity, kills double RAM. The correctness payoff
    (Windows host currently CPU-only, silent Whisper fallback) is roadmap-verified.
13. **B4 · #13 ClientSlot** (M, med): stream session stops holding the ParakeetClient RwLock
    write guard for the whole recording (typed Busy instead).

## Phase C — Settings hardening (roadmap Wave-4 = deep-review S2/D2)

14. **C1 · settings key constants (S2/#11-lite)** (M, low): `settings_keys` const module for
    the ~40 bare-string keys across 17 modules; mechanical swap; CI grep gate
    (`.get("`/`.set("` with a literal in src-tauri = fail) to stop regression.
15. **C2 · D2 decision — save_settings partial-merge (#9-adjacent)** (M, med, BEHAVIOR
    CHANGE): today TS AppSettings marks ~21 fields optional that serde requires — defended
    only by SettingsContext's full-spread. Options: (a) Rust-side read-merge-write partial
    save; (b) make TS fields required. Decide with Codex at phase start; (a) preferred but
    needs store-lock semantics; do NOT do both.
16. **C3 · defaults single-sourcing (S6)** (S, low): collapse the 3 Rust "auto"/mode default
    definitions; add the Rust↔TS voice-commands default-parity test (writing.ts:46 footgun).
17. **DEFER: full typed-settings module + versioned migration (#11+#10 heavy form) and
    tauri-specta codegen** — real fix, real churn; own plan when settings next hurt.

## Phase D — The monsters (highest risk, LAST, each its own smoke)

18. **D-1 · #20 stop_recording decomposition** (L, HIGH): extract the ~930-line spawned
    delivery task (audio.rs ~4962+) → `deliver_transcription(...)` + pure
    `delivery_disposition` with unit tests. Encodes 3 cancellation/stale-generation race
    rechecks — behavior-identical or STOP. Requires A1-A6 landed (delivery calls their
    modules). Characterization tests (plan 003) must pass unchanged.
19. **D-2 · start_recording slim** (L, med-high): validate→configure→arm helpers
    (audio.rs:3622-4340).
20. **D-3 · save_settings side-effects split** (M, med): persistence vs
    `apply_settings_side_effects` tail (settings.rs:573-983); ordering preserved.
21. **D-4 · lib.rs run() split** (M-L, med): state-init block → setup module;
    perform_startup_checks → startup.rs; .manage() ordering preserved exactly.

## Investigations (bounded, not slices)

- **I1 · D4-normalization asymmetry**: uploads/executor path skips normalizer.rs gain+dither
  that local recordings get. Run the WER harness A/B on real upload fixtures; only if it
  measurably hurts accuracy does a fix slice get authored. (Whisper-Metal is broken on the
  dev box — run via CI/other machine or Parakeet-only.)
- **I2 · roadmap Wave-5 leftovers** (#2+5 ModelCatalogContext, #27 error-surface, #23
  lifecycle statics, frontend S-batch): re-triage after Phase D; they block nothing.

## Sequencing vs plan 043 (Soniox streaming)

Independent. 043 needs NONE of this (correction §7: its enabler #14a folds into 043 itself).
Either order works; if 043 goes first, re-anchor A7 and B-phase line refs (they touch the
same audio.rs regions). Do NOT run both concurrently in the same files.

## Acceptance (every phase)

- Zero behavior delta (test suite passes UNCHANGED; no test edits except moves).
- Gates: cargo build/clippy -D warnings/test, pnpm typecheck/lint/test, windows-check green.
- audio.rs LOC checkpoint after Phase A (~5k) and Phase D (~3k target).
- Phase-end Codex review; desktop smoke (SMOKE.md protocol) after B3 and each D slice.
