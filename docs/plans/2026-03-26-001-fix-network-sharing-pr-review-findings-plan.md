---
title: "fix: Resolve All P1/P2 Issues in Network Sharing PR"
type: fix
status: active
date: 2026-03-26
origin: docs/reviews/2026-03-26-network-sharing-pr-review.md
---

# fix: Resolve All P1/P2 Issues in Network Sharing PR

## Overview

The `feature/network-sharing-remote-transcription` branch introduces remote transcription via network sharing (69 files, +18588/-229 lines). A 16-agent parallel code review identified 13 P1 (critical) and 9 P2 (major) issues that block merge. This plan addresses every finding so the branch is merge-ready.

## Problem Statement

The PR introduces a DOS vector (unbounded body), a panic path (UTF-8 byte slicing), a path traversal vulnerability, multiple state-machine ordering bugs, broken test isolation, and several frontend/documentation mismatches. None of these are regressions in existing code -- they are all in the new network-sharing feature code.

## Proposed Solution

Fix all 22 issues in-place on the feature branch. Group by file locality for parallel execution. No new features -- strictly correctness and safety fixes.

## Technical Approach

### Architecture

Fixes are grouped into 7 independent work streams that can execute in parallel. Dependencies are noted where they exist.

### Phase 1: Critical Rust Backend Fixes (Parallel)

These are the highest-impact fixes. Each targets a different file/module with no cross-dependencies.

---

#### Task 1A: Remote HTTP Safety (`src-tauri/src/remote/http.rs`)

**Issues:** P1#1 (DOS), P1#2 (panic), P1#4 (mutex contention)

**Changes:**

1. **Body size limit** (line 52): Add `warp::body::content_length_limit()` before `warp::body::bytes()`.
   ```rust
   // Before:
   .and(warp::body::bytes())

   // After:
   .and(warp::body::content_length_limit(50 * 1024 * 1024)) // 50MB max audio
   .and(warp::body::bytes())
   ```
   Define `const MAX_AUDIO_BODY_BYTES: u64 = 50 * 1024 * 1024;` at module top.

2. **UTF-8 safe truncation** (line 188-191): Replace byte slice with char-boundary-safe truncation.
   ```rust
   // Before:
   let preview = if response.text.len() > 100 {
       format!("{}...", &response.text[..100])
   } else {
       response.text.clone()
   };

   // After:
   let preview = if response.text.len() > 100 {
       let cut = response.text.char_indices()
           .nth(100)
           .map(|(i, _)| i)
           .unwrap_or(response.text.len());
       format!("{}...", &response.text[..cut])
   } else {
       response.text.clone()
   };
   ```

3. **Mutex contention** (lines 69, 125): Snapshot status metadata under lock, drop guard before transcription work.
   - In `handle_status` (line ~69): Lock, read status fields, drop guard, build response. This is likely already quick -- verify it doesn't hold lock while serializing.
   - In `handle_transcribe` (line ~125): Lock, extract auth/config/model info, drop guard, then perform transcription without holding the lock. This requires restructuring the handler to separate "read config" from "do work".
   - If `ServerContext` trait methods require `&mut self` for transcription, refactor to take config snapshot + separate transcription call. The trait may need a `transcribe_with_config(config, audio_data)` method that doesn't require the context lock.

**Acceptance:**
- [ ] Sending >50MB POST returns 413 Payload Too Large
- [ ] Transcribing text with multibyte characters at position 100 doesn't panic
- [ ] Status endpoint responds within 100ms while transcription is in progress

---

#### Task 1B: Remote Lifecycle Graceful Shutdown (`src-tauri/src/remote/lifecycle.rs`)

**Issue:** P1#3 (race on restart)

**Changes:**

1. Replace `try_bind_ephemeral` + manual `tokio::select!` shutdown with warp's graceful shutdown API:
   ```rust
   // Use bind_with_graceful_shutdown or try_bind_with_graceful_shutdown
   let (addr, server) = warp::serve(routes)
       .try_bind_with_graceful_shutdown(socket_addr, async move {
           shutdown_rx.await.ok();
       })?;
   ```

2. Store `JoinHandle<()>` for each spawned server task in `ServerHandle`.

3. Make `stop()` async: send shutdown signals, then await all `JoinHandle`s (with a 5-second timeout to prevent hanging).

4. In `start()`, await `stop()` completion before rebinding to ensure port is released.

**Acceptance:**
- [ ] Restart during active transcription completes in-flight request before rebinding
- [ ] No "address already in use" errors on rapid restart cycles
- [ ] `stop()` returns only after all server tasks have terminated

---

#### Task 1C: Command State Machine Ordering (`src-tauri/src/commands/remote.rs`)

**Issues:** P1#5 (wrong order), P1#6 (silent restore failure), P2 (sharing_was_active cleared early)

**Changes:**

1. **Validate before stopping** (lines 618-639): Reorder `set_active_remote_server`:
   ```
   Current: stop_sharing() -> validate_target() -> set_connection()
   Fixed:   validate_target() -> stop_sharing() -> set_connection()
   ```
   Specifically: call `ensure_remote_selection_is_allowed(&settings, id)?` before `manager.stop()`.

2. **Normalize empty model on restore** (lines 742-753): When `current_model` is empty string, auto-select first downloaded model (same behavior as `start_sharing`):
   ```rust
   let model_name = if current_model.is_empty() {
       // Auto-select best available model, matching start_sharing behavior
       resolve_first_available_model(&app_handle).await?
   } else {
       current_model.to_string()
   };
   ```

3. **Defer sharing_was_active clear** (lines 690-693): Move `settings.sharing_was_active = false` to after successful restore:
   ```rust
   // Before restore attempt: keep sharing_was_active = true
   // After successful manager.start():
   settings.sharing_was_active = false;
   settings.save()?;
   ```

**Acceptance:**
- [ ] Selecting an invalid remote server does not stop local sharing
- [ ] Restore with empty model auto-selects best available model
- [ ] Failed restore preserves `sharing_was_active` for retry

---

#### Task 1D: Path Traversal Validation (`src-tauri/src/commands/audio.rs`)

**Issue:** P1#7 (path traversal)

**Changes:**

1. Create a shared filename validation helper:
   ```rust
   fn validate_recording_filename(filename: &str) -> Result<(), String> {
       use std::path::Component;
       let path = std::path::Path::new(filename);

       // Reject absolute paths
       if path.is_absolute() {
           return Err("Absolute paths not allowed".to_string());
       }

       // Reject any non-Normal components (../, ./, prefix, root)
       for component in path.components() {
           match component {
               Component::Normal(_) => {}
               _ => return Err(format!("Invalid path component: {:?}", component)),
           }
       }

       Ok(())
   }
   ```

2. Apply validation in both `get_recording_path` (line ~3640) and `check_recording_exists` (line ~3628) before `recordings_dir.join(&filename)`.

3. After joining, canonicalize and verify the result starts with the recordings directory:
   ```rust
   let full_path = recordings_dir.join(&filename);
   let canonical = full_path.canonicalize()
       .map_err(|e| format!("Invalid recording path: {}", e))?;
   if !canonical.starts_with(&recordings_dir) {
       return Err("Path escapes recordings directory".to_string());
   }
   ```

**Acceptance:**
- [ ] `../../../etc/passwd` as filename returns error
- [ ] `/absolute/path` as filename returns error
- [ ] `valid-recording.wav` works normally
- [ ] Symlink escape is caught by canonicalization check

---

#### Task 1E: Startup Engine Validation (`src-tauri/src/lib.rs`)

**Issue:** P1#8 (engine bypass on startup)

**Changes:**

1. Replace the manual model-path resolution (lines 441-448) with the canonical validation helper:
   ```rust
   // Before:
   let model_path = if stored_engine == "whisper" {
       get_model_path(...)
   } else {
       PathBuf::new()  // BUG: empty path for non-whisper
   };
   manager.start(..., model_path, ...).await?;

   // After:
   match crate::commands::remote::resolve_shareable_model_config(
       &app_handle, &model_name, &stored_engine
   ).await {
       Ok((model_path, engine)) => {
           manager.start(..., model_path, model_name, engine).await?;
       }
       Err(e) => {
           warn!("Cannot restore sharing on startup: {}", e);
           // Clear sharing_was_active so we don't retry with bad config
       }
   }
   ```

2. Ensure `resolve_shareable_model_config` is `pub(crate)` accessible from `lib.rs`.

**Acceptance:**
- [ ] Startup with non-whisper engine does not start sharing server
- [ ] Startup with valid whisper model starts sharing correctly
- [ ] Startup with missing model logs warning and skips sharing

---

### Phase 2: Test Fixes (Parallel, Independent of Phase 1)

---

#### Task 2A: Integration Test Concurrency (`src-tauri/src/remote/integration_tests.rs`)

**Issues:** P1#9 (serialized concurrent test), P2 (wrong model path), P2 (weak assertions)

**Changes:**

1. **Fix concurrent test** (lines 726-727): Create separate transcription contexts for local and remote paths so they don't share a mutex:
   ```rust
   let remote_context = Arc::new(Mutex::new(RealTranscriptionContext::new(config.clone())));
   let local_context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
   // remote_context -> create_routes
   // local_context -> local transcription task
   ```

2. **Fix model path** (lines 68-76): Replace `com.voicetypr.app` with `com.ideaplexa.voicetypr`.

3. **Strengthen assertions** (lines 296-302): Use the existing `verify_transcription()` helper:
   ```rust
   assert!(
       verify_transcription(&transcribed_text),
       "Transcription should contain expected phrases, got: {}",
       transcribed_text
   );
   ```

**Acceptance:**
- [ ] Concurrent test exercises true parallelism (both paths run simultaneously)
- [ ] Model path matches actual app bundle ID
- [ ] Garbage transcription text fails the test

---

#### Task 2B: Unit Test Fixes (`src-tauri/src/tests/`)

**Issues:** P2 (IPv6 URLs, FIFO, latency, OS calls, machine-ID, timing)

**Changes:**

1. **IPv6 URLs** (`remote_client_tests.rs:225-228`): Fix URL construction in `src-tauri/src/remote/client.rs` to bracket IPv6 addresses (`http://[::1]:47842/...`), update test assertions to match.

2. **FIFO test** (`concurrent_tests.rs:314-315`): After `result.is_ok()`, also assert response status is success:
   ```rust
   if let Ok(response) = result {
       assert!(response.status().is_success(), "Expected 2xx, got {}", response.status());
       completion_order.lock().await.push(i);
   }
   ```

3. **Status latency bound** (`concurrent_tests.rs:376-382`): Add assertion:
   ```rust
   assert!(status_elapsed < Duration::from_secs(2),
       "Status should respond within 2s during transcription, took {:?}", status_elapsed);
   ```

4. **Mock OS calls** (`remote_commands_tests.rs:231-239`): Replace direct `open_firewall_settings()` call with a test that verifies intent without launching OS UI. Either: (a) make the command injectable via trait, (b) skip the test with `#[ignore]` and document it as integration-only, or (c) mock the system command.

5. **Machine-ID strictness** (`remote_commands_tests.rs:252-259`): Make assertions strict:
   ```rust
   let id = get_local_machine_id().expect("should return machine id");
   assert!(!id.is_empty(), "machine id should not be empty");
   ```

6. **Timing gates** (`logging_performance_tests.rs:303-309`): Replace hard `Duration::from_millis(1000)` assertion with a 10x relaxed CI-tolerant bound or mark as `#[ignore]` for CI. Example: `Duration::from_secs(10)` or use percentage-based relative threshold.

**Acceptance:**
- [ ] IPv6 URLs are valid HTTP URIs with brackets
- [ ] FIFO test fails on non-2xx responses
- [ ] Status latency is bounded
- [ ] `cargo test` doesn't open OS UI
- [ ] Machine-ID test fails on regression
- [ ] Timing tests don't flake on CI

---

#### Task 2C: Windows Test Script (`src-tauri/run-tests.ps1`)

**Issues:** P1#10 (CARGO_TARGET_DIR), P1#11 (incomplete binary patching)

**Changes:**

1. **Honor CARGO_TARGET_DIR** (line 49):
   ```powershell
   $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
   $depsDir = Join-Path $targetDir "debug" "deps"
   ```

2. **Patch all test binaries** (lines 45-50):
   ```powershell
   $testExes = Get-ChildItem -Path $depsDir -Filter "voicetypr*.exe" |
       Where-Object { $_.Name -notmatch "\.d$" -and $_.Name -match "^voicetypr(_lib)?-" }
   ```

**Acceptance:**
- [ ] Script works with `CARGO_TARGET_DIR=D:\cargo-target`
- [ ] Both `voicetypr-*.exe` and `voicetypr_lib-*.exe` are patched

---

### Phase 3: Frontend Fixes (Parallel, Independent of Phase 1-2)

---

#### Task 3A: Model Selection Remote Clearing (`src/components/sections/ModelsSection.tsx`)

**Issues:** P1#12 (cloud doesn't clear remote), P2 (removing active remote doesn't restore sharing)

**Changes:**

1. **Extract shared helper** for clearing remote selection:
   ```typescript
   const clearActiveRemoteServer = async () => {
     await invoke("set_active_remote_server", { serverId: null });
     setActiveRemoteServer(null);
   };
   ```

2. **Cloud path** (lines 595-608): Call `clearActiveRemoteServer()` before `onSelect(name)`.

3. **Remove active remote** (lines 269-283): When removing the currently active server, call `clearActiveRemoteServer()` before `remove_remote_server` to trigger backend restore flow.

**Acceptance:**
- [ ] Selecting a cloud model clears active remote server
- [ ] Removing active remote triggers sharing restore if was sharing

---

#### Task 3B: Frontend Event Handling (`src/components/`)

**Issues:** P2 (listener duplication, stats refresh, re-transcription UX, permission poll)

**Changes:**

1. **Event listener deduplication** (`AppContainer.tsx:107-124`): Split the effect into one-time event wiring (empty deps or stable refs) with proper cleanup:
   ```typescript
   useEffect(() => {
     const unlisteners: (() => void)[] = [];
     // Register events once
     const setup = async () => {
       unlisteners.push(await registerEvent("remote-server-error", handler));
       // ... other events
     };
     setup();
     return () => { unlisteners.forEach(fn => fn()); };
   }, []); // One-time only
   ```

2. **Overview stats refresh** (`TabContainer.tsx:80-83`): Add `transcription-updated` event handler:
   ```typescript
   registerEvent("transcription-updated", async () => {
     await loadHistory();
   });
   ```

3. **Permission poll** (`useAccessibilityPermission.ts:49-55`): Add silent polling mode:
   ```typescript
   const checkPermissionSilently = async () => {
     try {
       const result = await invoke<boolean>("check_accessibility_permission");
       setHasPermission(result);
     } catch { /* ignore in polling */ }
   };

   // Use silent check in interval, keep isChecking for manual checks only
   useEffect(() => {
     const interval = setInterval(checkPermissionSilently, 3000);
     return () => clearInterval(interval);
   }, []);
   ```

4. **Re-transcription UX** (`RecentRecordings.tsx:281-286`): Document current behavior (separate entry with new timestamp) or update to surface re-transcribed result more prominently. Minimum fix: sort re-transcribed entries to top of the list by checking `source_recording_id`.

**Acceptance:**
- [ ] Settings save doesn't duplicate error toast listeners
- [ ] Overview stats update after re-transcription
- [ ] Permission poll doesn't show loading indicator every 3s
- [ ] Re-transcribed results are discoverable

---

### Phase 4: Documentation and Scripts (Parallel, Independent)

---

#### Task 4A: Documentation Corrections

**Issues:** P1#13 (AGENTS.md), P2 (CLAUDE.md path, design doc claims)

**Changes:**

1. **AGENTS.md** (line 68): Fix worktree command:
   ```markdown
   # Before:
   git worktree add .worktrees/agent-<number> <branch-from-issue>

   # After:
   git worktree add .worktrees/agent-<number> -b agent-<number>-work origin/<branch-from-issue>
   ```
   This creates a local working branch tracking the remote, avoiding conflicts when the branch is already checked out.

2. **CLAUDE.md** (line 135): Fix model storage path from `com.voicetypr.app` to `com.ideaplexa.voicetypr`.

3. **Remote transcription design** (`docs/plans/2026-01-14-remote-transcription-design.md:105`): Correct "all models shared" to "active/selected model is shared".

4. **Save recordings design** (`docs/plans/2026-01-14-save-recordings-design.md:109`): Remove or annotate the history backfill claim as "not implemented".

**Acceptance:**
- [ ] AGENTS.md worktree command works when branch is already checked out
- [ ] CLAUDE.md paths match `tauri.conf.json` bundle ID
- [ ] Design docs match implemented behavior

---

#### Task 4B: Test Script Auth (`scripts/test-parallel-requests.ps1`)

**Issue:** P2 (missing auth header)

**Changes:**

1. Add optional `-ApiKey` parameter:
   ```powershell
   param(
       [string]$ServerUrl = "http://localhost:8765",
       [int]$NumRequests = 10,
       [string]$ApiKey = ""
   )
   ```

2. Build headers conditionally:
   ```powershell
   $headers = @{ "Content-Type" = "multipart/form-data" }
   if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
       $headers["X-VoiceTypr-Key"] = $ApiKey
   }
   ```

3. Pass headers in `Invoke-WebRequest` calls.

**Acceptance:**
- [ ] Script works without `-ApiKey` (open servers)
- [ ] Script sends auth header when `-ApiKey` is provided

---

#### Task 4C: Tray Menu and Settings (`src-tauri/src/menu/tray.rs`, `src-tauri/src/commands/settings.rs`)

**Issues:** P2 (stale active_remote_id), P2/P3 (model sort order), P2 (retention count null)

**Changes:**

1. **Stale active remote** (`tray.rs:81-84`): Derive effective active remote for tray rendering -- only treat `active_connection_id` as active if the connection passes `should_include_remote_connection_in_tray`. Otherwise treat as `None` for tray state.

2. **Model sort order** (`tray.rs:138-143`): Change from ascending to descending:
   ```rust
   // Before: a.2.cmp(&b.2)
   // After:  b.2.cmp(&a.2)
   ```

3. **Retention count null preservation** (`settings.rs:277-280`):
   - On load: distinguish missing key (use default 50) from explicit `null` (use `None` for unlimited).
   - On save: write `null` explicitly when `recording_retention_count` is `None`, don't delete the key.

**Acceptance:**
- [ ] Tray doesn't show stale remote selection for filtered connections
- [ ] Most accurate models appear first in tray
- [ ] Setting retention to "unlimited" (null) persists across app restart

---

## Execution Strategy

### Parallelization Map

All 4 phases can run in parallel since they touch different files:

```
Phase 1: [1A] [1B] [1C] [1D] [1E]  -- 5 parallel Rust backend tasks
Phase 2: [2A] [2B] [2C]            -- 3 parallel test tasks
Phase 3: [3A] [3B]                  -- 2 parallel frontend tasks
Phase 4: [4A] [4B] [4C]            -- 3 parallel docs/scripts/tray tasks

Total: 13 independent tasks, all parallelizable
```

**Exception:** Task 1A (mutex fix in http.rs) and Task 2A (concurrent test fix in integration_tests.rs) are logically related -- the test validates the fix. But they touch different files and can be implemented in parallel; verification happens after both complete.

### Verification After All Tasks

After all parallel tasks complete:

1. `cargo check` in `src-tauri/` -- ensures Rust compiles
2. `cargo test` in `src-tauri/` -- ensures tests pass (skip `#[ignore]` integration tests)
3. `pnpm typecheck` -- ensures TypeScript compiles
4. `pnpm lint` -- ensures ESLint passes
5. `pnpm test` -- ensures frontend tests pass
6. Manual review of each fix against its acceptance criteria

## Risk Analysis

| Risk | Mitigation |
|------|------------|
| Mutex refactor in http.rs changes `ServerContext` trait | Keep trait backward-compatible; add methods, don't remove |
| Graceful shutdown timeout hangs | Use `tokio::time::timeout(Duration::from_secs(5), handle.await)` |
| Path validation too strict for edge cases | Test with actual recording filenames from existing data |
| IPv6 bracket fix breaks existing client connections | Only affects URL formatting, not actual binding |
| Frontend event cleanup breaks existing listeners | Test with full app flow: start sharing -> change settings -> verify events |

## Sources & References

- **Origin document:** [docs/reviews/2026-03-26-network-sharing-pr-review.md](docs/reviews/2026-03-26-network-sharing-pr-review.md) -- complete review with all 22 issues
- Warp body limits: `warp::body::content_length_limit()` API
- Rust string safety: `str::char_indices()` for UTF-8 boundary-safe slicing
- Tauri capabilities: `src-tauri/capabilities/` for permission changes
- App bundle ID: `src-tauri/tauri.conf.json` (`com.ideaplexa.voicetypr`)
