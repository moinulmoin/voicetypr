# Plan 022: Wave 1 (F3) — save uploaded transcript to .txt/.md

> **STATUS: IN PROGRESS — claimed Main 2026-06-16.** Wave 1 of `plans/021-v2-feature-track.md`. Executable, small.

## Status
- **Priority**: P2 · **Effort**: S · **Risk**: LOW
- **Depends on**: none (ships on the existing upload result)
- **Parent**: plans/021 (Wave 1)
- **Planned at**: 2026-06-16

## Scope
Add a **Save** action to the Upload result that writes the transcript to a user-chosen `.txt` OR `.md` file via a native save dialog. **Both formats ship in this cut** — `.txt` = raw transcript; `.md` = the transcript under a minimal `# <name>` heading (no speaker blocks). Only the *speaker-block* `.md` enrichment is deferred to Wave 2/F2.

## Approach
- **Backend** (`src-tauri/src/commands/utils.rs`, beside `export_transcriptions`): `#[tauri::command] save_transcript_file(path: String, content: String) -> Result<(), String>` — reject empty path/content, `std::fs::write`, log, `Ok(())`. Register in `src-tauri/src/lib.rs` `invoke_handler` (same list as `export_transcriptions`).
- **Frontend** (`src/components/sections/AudioUploadSection.tsx`): a **Save** button in the result actions row (beside Copy). Handler opens `save()` (`@tauri-apps/plugin-dialog`) with a default filename from the source basename + filters `[Text .txt, Markdown .md]`. Build content by the **chosen extension**: `.md` → `# ${base}\n\n${resultText}`; otherwise raw `resultText`. Then `invoke('save_transcript_file', { path, content })`; success/error toast; cancel = no-op.
- **Note**: frontend fs is `fs:scope-appdata` (`capabilities/macos.json`), so the write MUST go through the backend command, NOT `plugin-fs` `writeTextFile` to the chosen path.
- **Deferred to Wave 2/F2**: speaker-attributed `.md` (Speaker 1 / Speaker 2 blocks). This cut is plain text in both formats.

## Tests
- Backend unit test: `save_transcript_file` writes given content to a temp path; assert the file matches; clean up. Match repo test conventions (`#[tokio::test]` for async).

## Acceptance
- `save_transcript_file` compiles + is registered; backend test passes.
- Upload result shows a Save button → native dialog (txt/md filters) → chosen path receives the transcript (`.md` gets the `# <name>` heading); cancel = no-op; toasts on success/failure.

## Smoke (append to SMOKE.md when landed)
- **022-S1**: upload → transcribe → Save → pick `.txt` then `.md` → both files contain the transcript (`.md` has the heading); cancel dialog = no write.

## STOP
- If `dialog:default` does not include the save permission at runtime, add `dialog:allow-save` to the capabilities — do NOT broaden the `fs` scope.
