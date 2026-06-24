# Plan 012: Design the shared transcription contract (V2 Phase 1) — design/spike, no production code

> **Executor instructions**: Follow this plan step by step. This is a DESIGN
> plan: the deliverable is a design document, not code changes. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/transcription.rs src-tauri/src/remote/ src-tauri/src/cli.rs src-tauri/src/commands/audio.rs`
> Drift here does NOT stop the plan (a design doc absorbs drift) — but every
> file:line citation you write must come from the LIVE code, not from this
> plan's excerpts.

## Status

- **Priority**: P2
- **Effort**: M (the design; the implementation it specifies will be L)
- **Risk**: LOW (no production code changes allowed)
- **Depends on**: none (but read plans 002/006 outcomes if already landed —
  they touch the same seams)
- **Category**: direction
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

The V2 roadmap's stated core (`docs/plans/2026-05-12-v2-roadmap.md:25,51-65`)
is **one shared transcription contract** — one canonical request/response
model, one execution path, a stable error taxonomy — consumed by desktop
recording, file upload, the network-sharing server, and the CLI. Today only
fragments exist: `src-tauri/src/transcription.rs` defines
`TranscriptionJob`/`TranscriptionResult` DTOs, but there is **no request
envelope, no error taxonomy**, and each surface rebuilds routing and settings
reads itself. Every week this drifts further: the remote HTTP protocol ships
context in a base64 header, the CLI grew its own remote path, and error
shapes are `String` vs `TranscriptionFailure` vs `RemoteClientError`. The
design must land before more surfaces harden. This plan produces the
decision-complete design doc that a follow-up implementation plan (or series)
can execute mechanically.

## Current state (the fragments to unify — verify each against live code)

- `src-tauri/src/transcription.rs` (186 lines, the would-be contract home):
  `TranscriptionSource` (DesktopRecording | AudioFile | AudioBytes |
  RemoteServer), `TranscriptionTask` (Transcribe | TranslateToEnglish),
  `TranscriptionJob` (`:36-61`, incl. `from_legacy_settings`),
  `TranscriptionSegment`, `TranscriptionTimings`, `TranscriptionResult`
  (`:78-127`). No request envelope, no error type.
- Local desktop path: `commands/audio.rs:3178-3396` — a `match` over
  `ActiveEngineSelection` (Whisper | Parakeet | Cloud/Soniox | Remote) inside
  the spawned transcription task; errors are
  `TranscriptionFailure::{Local(String), Remote(RemoteClientError)}`;
  settings (language/translate) read and normalized locally (`:1278` area).
- Upload path: `transcribe_audio_file` / `transcribe_audio_file_for_cli`
  (`commands/audio.rs:4128-4145`) — separate impl entry, builds remote
  requests via a local helper (`:704` area).
- Remote server: `remote/transcription.rs:205-258` implements `ServerContext`;
  `transcribe_inner` (`:261-333`) REBUILDS a `TranscriptionJob` from shared
  state + headers, writes bytes to a temp WAV, and re-implements engine
  routing (`match engine.as_str()`). Errors: `String`, returned verbatim to
  LAN clients (`remote/http.rs:497-505`).
- Remote wire protocol: `remote/http.rs:18-26,396-455` — raw audio body;
  metadata in headers (`X-Voicetypr-Key`, `X-Voicetypr-Context` base64,
  language/task as query/headers — verify exact extraction at `:90-110`).
  The deferred-items plan (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:82-95`)
  already specifies the successor: multipart audio + JSON metadata,
  advertised capabilities, byte-limited request-local context, no
  persistence of Personal Library terms.
- Remote client: `remote/client.rs` — `TranscriptionRequest` (`:72-116`),
  `RemoteClientError` (`:135-245`, the only structured error taxonomy in the
  system), timeout policy (`:296-356`).
- CLI: `cli.rs` — `Status`/`Models`/`Transcribe`/`Record` (`:38-44`), its own
  remote path `transcribe_via_remote` (`:342-392`).
- Capability matrix: `src-tauri/src/provider_capabilities.rs` (6.4 KB) —
  engine capabilities incl. `shareable_remote` (used at
  `commands/remote.rs:70-80`).

## Commands you will need

| Purpose              | Command                                          | Expected on success      |
|----------------------|--------------------------------------------------|--------------------------|
| Citation spot-checks | `grep -n "<symbol>" src-tauri/src/<file>`        | line matches your citation |
| Doc renders          | n/a (markdown)                                   | —                        |

## Scope

**In scope** (the only files you may create/modify):
- `docs/plans/2026-06-10-shared-transcription-contract-design.md` (create —
  matches the repo's existing design-doc naming in `docs/plans/`)
- `plans/README.md` (status row)

**Out of scope — HARD RULE for this plan**:
- ANY change under `src/` or `src-tauri/` (including "harmless" DTO
  additions). The deliverable is the document.

## Git workflow

- Branch: `advisor/012-transcription-contract-design`
- Commit message: `docs: add shared transcription contract design`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Inventory (the evidence tables)

Produce three tables in the doc, every row cited `file:line` from live code:

1. **Request shapes**: for each surface (desktop recording, upload, remote
   server inbound, remote client outbound, CLI transcribe, CLI record) — how
   audio, engine, model, language, task, and context enter, and where
   settings are read.
2. **Error taxonomies**: every error type/shape per surface
   (`String`, `TranscriptionFailure`, `RemoteClientError`, HTTP
   `ErrorResponse`), including which leak internals to LAN clients.
3. **Duplicated logic**: engine routing implementations (count them),
   settings normalization sites, temp-file handling sites.

**Verify**: every row has a citation; spot-check 5 rows with grep.

### Step 2: The contract proposal

Specify, with full Rust type sketches (in the doc, NOT in code):

- `TranscriptionRequest`: source, audio (`Path` | `Bytes`), engine/model
  selection (explicit vs "host default" for remote), language, task,
  request-local context (byte-capped), cancellation handle, timeout policy.
- `TranscriptionError`: stable `code` enum (e.g. `model_unavailable`,
  `engine_failed`, `audio_invalid`, `cancelled`, `timeout`,
  `transport_failed`, `unauthorized`), `retryable: bool`, `user_message`,
  internal `detail` (never serialized to remote clients — this subsumes
  plan-006's documented lock/leak limitations and SECURITY-04 from the audit).
- The single executor seam: one function/trait that all four surfaces call;
  where it lives (proposal: `transcription/` module with `transcription.rs`
  DTOs as today's seed); how `ActiveEngineSelection` and
  `provider_capabilities` fold in.
- Wire mapping: how the contract serializes onto the multipart protocol from
  the deferred-items plan (`:82-95`), and what `/api/v1/status` capability
  advertisement adds; explicitly state the compatibility decision (clean
  cutover vs version negotiation) with a recommendation grounded in whether
  remote has shipped to users (check release notes/`git tag` for the feature;
  cite).
- In-flight semantics: what happens to running jobs on model change
  (the plan-006 maintenance note's open question) — pick a rule
  (recommendation: jobs complete on the snapshot they started with).

### Step 3: Migration map

Ordered, mechanically-executable stages, each leaving the build green:
(1) add DTOs + executor beside existing code; (2) port local recording path;
(3) port upload + CLI local; (4) port remote server inbound (+ multipart);
(5) port remote client + CLI remote; (6) delete `TranscriptionFailure`,
ad-hoc structs, header-context path. For each stage: files touched, tests
that pin it (name existing suites: `remote_http_tests`, `remote_client_tests`,
`remote_transcription_tests`, `transcription_history`, plan-003's module),
and a rollback note.

### Step 4: Open questions for the maintainer

Max 5, each with a recommended answer. Must include: clean-cutover vs
compat for the remote protocol; whether CLI output schema freezes now
(direction finding C from the audit); whether `TranscriptionFailure::Remote`
retry-from-history semantics (`commands/audio.rs:3740-3802`) generalize to
all retryable errors.

### Step 5: Index update

Update this plan's row in `plans/README.md` to DONE with a pointer to the
doc.

**Verify**: doc exists at the path above; contains the three inventory
tables, type sketches, 6-stage migration map, ≤5 open questions;
`git status` shows only the two in-scope files.

## Test plan

Not applicable (docs only). Quality gate for the doc: a reviewer can start
implementation stage 1 from the doc alone without re-reading this plan.

## Done criteria

ALL must hold:

- [ ] `docs/plans/2026-06-10-shared-transcription-contract-design.md` exists
      with sections: Inventory (3 tables), Contract, Wire mapping, In-flight
      semantics, Migration map (6 stages), Open questions (≤5)
- [ ] Every factual claim about current code carries a live `file:line`
      citation
- [ ] No file under `src/` or `src-tauri/` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- You cannot determine whether the remote feature has shipped to end users
  (needed for the cutover-vs-compat recommendation) — ask the operator
  instead of guessing.
- The live code has drifted so far from the Current state map that the
  inventory would exceed ~2 days of reading — report scope before continuing.

## Maintenance notes

- This doc supersedes the contract-shaped fragments of
  `docs/plans/2026-06-07-001-v2-deferred-items-plan.md` items it absorbs —
  the doc should say which items those are (the audit found item 6, Voice
  Commands, is already shipped; note that staleness too).
- The implementation stages should land as separate advisor plans (013+) or
  feature branches, each gated on the prior stage's tests.
