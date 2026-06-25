# Plan 025: Wave 4 (F1) — CLI agent polish (MCP deferred)

> **STATUS: IN PROGRESS — claimed Main 2026-06-16.** Wave 4 of `plans/021-v2-feature-track.md`. MCP server deferred (on-demand, separate future plan).

## Status
- **Priority**: P2 · **Effort**: S · **Risk**: LOW
- **Depends on**: W0/W2 (`UploadTranscription` with text/words/metadata)
- **Parent**: plans/021 (Wave 4)

## Scope
Make the existing CLI a clean local-agent surface: `--json` consistent across all subcommands, and `transcribe`/`record` emit the structured transcription artifact. **MCP server is deferred** (the CLI already covers shell-invoking agents; build MCP only when an MCP-native host needs it — stdio + 127.0.0.1-only-tokened if ever HTTP, never the LAN server).

## Approach (done)
- `src-tauri/src/cli.rs`: `run_status` + `run_models` honor `--json` (pretty JSON with the flag; concise human-readable output without — `format_availability` helper summarizes engine availability). `run_transcribe` + `run_record` local branches build payloads `{ text, words, metadata, model, engine[, stop_reason] }`.
- `src-tauri/src/commands/audio.rs`: `transcribe_audio_file_for_cli` returns `UploadTranscription` (was `String`); both cli.rs callers updated. Live GUI `transcribe_audio_file` command + frontend untouched.

## Tests
- `cli::tests`: `--json` flag parsing for status/models; `format_availability` (none / lists engines / cloud-requires-ready).

## Acceptance
- All four subcommands honor `--json` (human default, JSON with flag); `transcribe --json` / `record --json` include `words` + `metadata` alongside `text`/`model`/`engine`; live GUI path unchanged; gates green.

## Smoke (append to SMOKE.md)
- **025-S1**: `voicetypr transcribe --file x.wav --json` emits `{ text, words, metadata, model, engine }`; without `--json` prints just the text. `voicetypr status` / `models` print human-readable output by default and JSON with `--json`. (Flag parsing + availability formatting are unit-covered; the real transcription round-trip is the residue.)

## STOP
- None expected (CLI-only, additive).
