# Plan 010: Stop full-scanning the transcription history store on every read and save

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/commands/audio.rs src-tauri/src/tests/transcription_history.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (history ordering, retry reconciliation, and the duplicate-
  save guard are user-visible; behavior must be preserved except cost)
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

The transcription store (`tauri-plugin-store`, file `transcriptions`, one
entry per transcription keyed by RFC3339 timestamp) is scanned in full on hot
paths:

- **Every paged read** (`get_transcription_history`, default `limit=50`)
  clones and reconciles EVERY value, sorts everything, then truncates. The
  history tab and startup pay O(N) value-clones for a 50-row page.
- **Every save** runs a duplicate-check that iterates all keys and reads
  values to find the latest entry.

Users who keep months of dictation history pay this on every single
transcription. Keys are timestamps, so ordering is available from keys alone
— pages can be selected before touching values.

## Current state

All in `src-tauri/src/commands/audio.rs`.

Save-path duplicate check (`:3945-3987`, inside the save function — locate
the enclosing fn with `grep -n "fn save_transcription" src-tauri/src/commands/audio.rs`):

```rust
    if let Ok(store) = app.store("transcriptions") {
        ...
        for key in store.keys() {            // :3948 — full scan
            ...                              // finds `latest` (ts, value)
        }
        if let Some((ts, v)) = latest {
            let same_text = v.get("text")...== text;
            let same_model = v.get("model")...== model;
            let within_window = ...parse ts, |now - ts| <= 2s...;
            if same_text && same_model && within_window {
                log::info!("Skipping duplicate transcription save ...");
                return Ok(());
            }
        }
    }
```

Then the save itself keys by timestamp (`:3990-3999`):

```rust
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut transcription_data = serde_json::json!({
        "text": text.clone(), "model": model, "timestamp": timestamp.clone()
    });
```

Paged read (`:4072-4118`):

```rust
pub async fn get_transcription_history(app: AppHandle, limit: Option<usize>)
    -> Result<Vec<serde_json::Value>, String>
{
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;
    let current_session_marker = current_retranscription_session_marker();
    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
    let mut pending_updates: Vec<(String, serde_json::Value)> = Vec::new();

    for key in store.keys() {                                 // full scan
        if let Some(value) = store.get(&key) {                // clones every value
            let reconciled = reconcile_transcription_history_entry(
                value.clone(), &current_session_marker);
            if reconciled != value {
                pending_updates.push((key.to_string(), reconciled.clone()));
            }
            entries.push((key.to_string(), reconciled));
        }
    }
    if !pending_updates.is_empty() { ...store.set/save + tray update... }
    entries.sort_by(|a, b| b.0.cmp(&a.0));                    // sorts everything
    let limit = limit.unwrap_or(50);
    entries.truncate(limit);
    Ok(entries.into_iter().map(|(_, v)| v).collect())
}
```

`get_transcription_count` (`:4122-4126`) is already cheap (`keys().len()`).

`reconcile_transcription_history_entry` is a helper near these commands
(locate with grep) that rewrites stale "retranscribing" rows from dead
sessions; existing tests for history live in
`src-tauri/src/tests/transcription_history.rs` (4.3 KB — read it; model new
tests on it).

Key fact making the fix cheap: **keys are RFC3339 UTC timestamps**, so
lexicographic descending key order == newest-first. `store.keys()` returns
keys without cloning values.

## Target design

Preserve the public command contracts exactly. Two localized changes:

**A. Paged read — sort keys first, touch only the page.**

```rust
    let mut keys: Vec<String> = store.keys().map(|k| k.to_string()).collect();
    keys.sort_unstable_by(|a, b| b.cmp(a));        // newest first, keys only
    let limit = limit.unwrap_or(50);

    let mut entries = Vec::with_capacity(limit.min(keys.len()));
    let mut pending_updates: Vec<(String, serde_json::Value)> = Vec::new();
    for key in keys.into_iter().take(limit) {
        if let Some(value) = store.get(&key) {
            let reconciled = reconcile_transcription_history_entry(
                value.clone(), &current_session_marker);
            if reconciled != value {
                pending_updates.push((key.clone(), reconciled.clone()));
            }
            entries.push(reconciled);
        }
    }
    ...existing pending_updates persistence + tray update, unchanged...
    Ok(entries)
```

Behavior delta (accepted, document in maintenance notes): stale rows BEYOND
the requested page are reconciled lazily when they scroll into a page,
instead of eagerly on every read. Reconciliation is idempotent, so this is
safe.

**B. Save-path duplicate check — max key, single value read.**

```rust
    if let Ok(store) = app.store("transcriptions") {
        let latest_key = store.keys().map(|k| k.to_string()).max(); // O(N) over keys, no values
        if let Some(key) = latest_key {
            if let Some(v) = store.get(&key) {
                ...existing same_text/same_model/within_window checks,
                   using `key` as the timestamp string...
            }
        }
    }
```

(Replicates the old `latest` selection exactly when keys are well-formed
timestamps; malformed keys sort low and were never selected as `latest`
anyway unless the store was empty of valid ones — preserve the old code's
tolerance by keeping the checks `unwrap_or(false)`.)

## Commands you will need

| Purpose            | Command                                                  | Expected on success |
|--------------------|----------------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                            | exit 0              |
| History tests      | `cd src-tauri && cargo test transcription_history`       | all pass            |
| Full backend tests | `cd src-tauri && cargo test`                             | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                      | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/commands/audio.rs` — `get_transcription_history` body and
  the save-path duplicate check only.
- `src-tauri/src/tests/transcription_history.rs` — new tests.

**Out of scope**:
- `reconcile_transcription_history_entry` logic.
- Export (`src-tauri/src/utils.rs` materializes all history — intentional
  all-rows path; leave it).
- `get_transcription_count`, delete/retry commands, store file format, key
  format.
- Frontend (`src/hooks/useTranscriptionHistory.ts`) — contract unchanged.

## Git workflow

- Branch: `advisor/010-history-paged-reads`
- Commit message: `perf: page transcription history reads by key order`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Read the existing history tests and helpers

Read `src-tauri/src/tests/transcription_history.rs` fully and locate
`reconcile_transcription_history_entry` + `current_retranscription_session_marker`
in `commands/audio.rs`. Note which behaviors are already pinned.

**Verify**: report the list.

### Step 2: Apply change A (paged read)

Rewrite `get_transcription_history` per the target design. Keep the
`pending_updates` persistence block and tray-update call byte-identical.

**Verify**: `cargo check` exit 0; `cargo test transcription_history` passes.

### Step 3: Apply change B (save dup-check)

Rewrite the duplicate-check block per the target design. The
`within_window`/`same_text`/`same_model` logic moves over unchanged.

**Verify**: `cargo check` exit 0; `cargo test` passes.

### Step 4: New tests

In `src-tauri/src/tests/transcription_history.rs`, following its existing
style (it exercises pure helpers; if it has store-backed tests, follow that
harness):

1. `page_returns_newest_first_limited`: build ≥ 5 entries with ascending
   timestamp keys; request limit 2 → exactly the 2 newest, newest first.
2. `reconciliation_applies_to_returned_page`: a stale "retranscribing" row
   from a dead session inside the page is reconciled in the returned values.
3. `stale_rows_outside_page_untouched`: stale row OUTSIDE the page remains
   unreconciled in the store after the call (documents the new laziness).
4. `duplicate_save_guard_uses_latest_entry`: with entries where the
   lexicographically-max key is the newest, the guard skips a same-text/
   same-model save within 2 s and allows one after the window (test the
   extracted comparison if direct command testing needs an AppHandle — in
   that case extract the comparison into a pure
   `fn is_duplicate_transcription(latest_key: &str, latest: &Value, text: &str, model: &str, now: DateTime<Utc>) -> bool`
   and unit-test THAT; the extraction is in scope).

**Verify**: `cargo test transcription_history` → all new tests pass.

### Step 5: Full suite + format

**Verify**: `cargo test` all pass; `cargo fmt --check` exit 0.

## Test plan

Tests 1-4 above; tests 1-3 require constructing a store — if the existing
test file has no store harness, use the pure-function extraction route for
ALL of them: extract `fn page_history_keys(keys: Vec<String>, limit: usize) -> Vec<String>`
and test key-ordering/truncation purely, plus the reconcile behavior through
the already-pure `reconcile_transcription_history_entry`. State in your
report which route you took.

## Done criteria

ALL must hold:

- [ ] `get_transcription_history` no longer calls `store.get` for keys
      outside the requested page (quote the new loop in the report)
- [ ] Save-path dup-check no longer reads values in a loop (single
      `store.get(&max_key)`)
- [ ] New tests pass; full `cargo test` passes
- [ ] `cargo fmt --check` exit 0
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- History keys turn out NOT to be uniformly timestamp-formatted (grep the
  save/retry/import paths for `store.set(` on `transcriptions` — if any
  writer uses a non-timestamp key, the key-ordering assumption breaks: STOP).
- `reconcile_transcription_history_entry` has side effects beyond returning a
  value (it must be pure for the lazy-page change to be safe).
- Any existing test pins the eager-reconcile-everything behavior.

## Maintenance notes

- Reconciliation is now lazy (page-scoped). If a future feature needs
  store-wide reconciliation (e.g. badge counts of failed rows), do it once at
  startup, not per read.
- If history grows into tens of thousands of rows, the next step is a real
  index/manifest or SQLite — out of scope until measured.
- Reviewer should scrutinize: `take(limit)` interacting with `store.get`
  misses (deleted-but-listed keys): the old code pushed only existing values;
  the new loop must not pad the page with fewer entries silently differently
  than before (both behaviors match: missing keys are skipped).
