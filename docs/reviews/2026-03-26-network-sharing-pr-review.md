# Code Review: feature/network-sharing-remote-transcription

**Date:** 2026-03-26
**Reviewer:** Agent 16 (parallel review with 15 peers)
**Branch:** `feature/network-sharing-remote-transcription` → `main`
**Files Changed:** 69 (+18588/-229 lines)

## Verdict: ❌ NOT READY TO MERGE

**22 critical/major issues identified.** Multiple correctness bugs, broken state transitions, incorrect test assertions, and documentation that contradicts implementation.

---

## Critical Issues (P1)

Issues that must be fixed before merge.

### Rust Backend: Remote Module

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 1 | **Memory exhaustion DOS**: `/api/v1/transcribe` buffers entire request before validation. Client can send unlimited POST to exhaust server memory. | `src-tauri/src/remote/http.rs:48-52` | DOS vulnerability |
| 2 | **Byte-slice panic**: `&response.text[..100]` slices on byte offset. Multibyte characters at boundary cause panic, turning success into crash. | `src-tauri/src/remote/http.rs:186-187` | Server crash |
| 3 | **Race on server restart**: Shutdown drops listener immediately without drain. Port rebind races, in-flight requests killed. | `src-tauri/src/remote/lifecycle.rs:193-200` | Request loss |
| 4 | **Status blocked by transcription**: Both endpoints share same mutex. Status blocks behind active transcription for seconds/minutes. | `src-tauri/src/remote/http.rs:123-125` | Health check unresponsive |

### Rust Backend: Commands

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 5 | **Wrong order in set_active_remote_server**: Stops sharing before validating remote target. Failed remote selection unexpectedly tears down local sharing. | `src-tauri/src/commands/remote.rs:632-639` | UX regression |
| 6 | **Auto-model restore fails silently**: Empty `current_model` fed to restore path returns `Ok(())` on failure. Sharing never restores on auto-select setups. | `src-tauri/src/commands/remote.rs:742-751` | Sharing won't restore |
| 7 | **Path traversal in recording helpers**: `get_recording_path` trusts caller-supplied filename. `..`/absolute paths escape recordings directory. | `src-tauri/src/commands/audio.rs:3640-3646` | File system probe |

### Rust Backend: Integration

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 8 | **Wrong engine accepted on startup**: Non-whisper engines bypass validation. Server starts with empty model path. | `src-tauri/src/lib.rs:441-448` | Silent failure |
| 9 | **Concurrent test is serialized**: Both paths lock same mutex. Test never exercises true concurrency—Issue #3 remains undetected. | `src-tauri/src/remote/integration_tests.rs:726-727` | Issue #3 uncaught |

### Rust Config/Build

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 10 | **`CARGO_TARGET_DIR` ignored**: Script hardcodes `target\debug\deps`. Won't find binaries in non-default locations. | `src-tauri/run-tests.ps1:49` | Windows CI fails |
| 11 | **Only patches lib test binary**: Main test binary (`voicetypr-*.exe`) not patched. Still crashes. | `src-tauri/run-tests.ps1:45-46` | Windows workaround incomplete |

### Frontend

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 12 | **Cloud models ignore active remote**: Cloud branch doesn't clear `activeRemoteServer`. Backend prefers remote over model. | `src/components/sections/ModelsSection.tsx:605-608` | Cannot switch to cloud |

### Documentation

| # | Issue | File:Line | Impact |
|---|-------|-----------|--------|
| 13 | **AGENTS.md worktree setup broken**: `<branch-from-issue>` fails when branch already checked out elsewhere. Multi-agent workflow fails. | `AGENTS.md:68` | Protocol broken |

---

## Major Issues (P2)

Should be fixed before merge.

### Rust Tests

- **IPv6 URLs unbracketed**: Tests accept `http://::1:47842/...` but unbracketed IPv6 literals aren't valid HTTP. Client connections silently fail. (`remote_client_tests.rs:225-228`)
- **FIFO test records 4xx as success**: `if result.is_ok()` treats HTTP 401/415/500 as completion. Regression passes test. (`concurrent_tests.rs:314-315`)
- **Status responsiveness has no bound**: Test logs latency but doesn't assert. Passes even with 10s delay behind transcription. (`concurrent_tests.rs:376-382`)
- **Unit test opens real firewall**: `open_firewall_settings()` shells out to `open`/`control`. Pops OS UI during `cargo test`. (`remote_commands_tests.rs:231-239`)
- **Machine-ID tests accept errors**: Treat `Err(_)` as acceptable. Don't fail when helper regresses. (`remote_commands_tests.rs:252-259`)
- **Hard wall-clock timing gates**: `concurrent_logging_performance`, `memory_efficiency`, `error_logging_under_stress` fail on scheduler variance. Flaky CI. (`logging_performance_tests.rs:303-309`)
- **Wrong model path**: `com.voicetypr.app` vs actual `com.ideaplexa.voicetypr`. Tests miss real model, redownload. (`integration_tests.rs:68-76`)
- **Transcript assertions only check non-empty**: Garbage text passes. Doesn't validate correctness. (`integration_tests.rs:296-302`)

### Rust Commands/Settings

- **`sharing_was_active` cleared before restore**: Failure leaves both memory and settings inconsistent. (`commands/remote.rs:691-693`)
- **`recording_retention_count: null` not preserved**: Round-trips to default instead of "unlimited". (`commands/settings.rs:277-280`)

### Rust Tray Menu

- **SelfConnection filter leaves stale `active_remote_id`**: No way to clear remote selection from tray when entry filtered. (`menu/tray.rs:81-84`)
- **Model sort is ascending accuracy**: Least accurate models float to top. Opposite of UI auto-selection. (`menu/tray.rs:138-143`)

### Frontend

- **Removing active remote doesn't restore sharing**: `remove_remote_server` removes connection but doesn't restore local sharing. (`sections/ModelsSection.tsx:272-276`)
- **Remote error listener registered repeatedly**: Each settings save adds another listener. Multiplies toasts/notifications. (`AppContainer.tsx:107-124`)
- **Overview stats don't refresh after re-transcription**: `transcription-updated` event not handled in TabContainer. (`tabs/TabContainer.tsx:80-83`)
- **Re-transcribed results stay under old timestamp**: Buried in history instead of surfacing as new activity. (`sections/RecentRecordings.tsx:281-286`)
- **Permission poll toggles `isChecking`**: Surfaces as active check to user every 3 seconds. (`hooks/useAccessibilityPermission.ts:49-55`)

### Documentation

- **Model storage path wrong**: `com.voicetypr.app` vs actual `com.ideaplexa.voicetypr`. (`CLAUDE.md:135`)
- **Design says "all models shared"**: Only single model supported. (`2026-01-14-remote-transcription-design.md:105`)
- **Design claims history backfill on cleanup**: Doesn't exist. (`2026-01-14-save-recordings-design.md:109`)

### Scripts

- **Parallel request script missing auth**: Never sends `X-VoiceTypr-Key` header. Fails against password-protected servers. (`test-parallel-requests.ps1:49-53`)

---

## Minor Issues (P3)

- Tray model sort order contradicts comment (`menu/tray.rs:138-143`)

---

## Verdict by Area

| Area | Verdict | P1 | P2 | P3 |
|------|---------|-----|-----|-----|
| Rust: Remote Module Core | ❌ | 4 | 1 | 0 |
| Rust: Commands | ❌ | 3 | 2 | 0 |
| Rust: Lib/Integration | ❌ | 1 | 2 | 1 |
| Rust: Tests | ❌ | 1 | 8 | 1 |
| Rust: Config/Build | ❌ | 2 | 2 | 0 |
| Rust: Tauri Config | ✅ | 0 | 0 | 0 |
| Frontend: Remote Components | ⚠️ | 0 | 1 | 0 |
| Frontend: Settings/Models | ❌ | 1 | 2 | 0 |
| Frontend: Tabs/History | ⚠️ | 0 | 2 | 0 |
| Frontend: Types/Hooks | ⚠️ | 0 | 1 | 0 |
| Frontend: Tests | ✅ | 0 | 0 | 0 |
| Documentation | ❌ | 1 | 3 | 0 |
| Scripts | ⚠️ | 0 | 1 | 0 |
| Gitignore/Config | ✅ | 0 | 0 | 0 |

---

## Recommended Actions

### Before Merge (P1 issues)

1. **Remote HTTP module**: Add request body size limit, fix Unicode handling, implement graceful shutdown, separate status mutex
2. **Commands**: Fix state transition order in `set_active_remote_server`, handle auto-model restore, add path validation
3. **Lib init**: Validate engine before starting sharing
4. **Integration test**: Make concurrent test actually concurrent
5. **run-tests.ps1**: Honor `CARGO_TARGET_DIR`, patch all test binaries
6. **Frontend**: Clear `activeRemoteServer` in cloud branch
7. **AGENTS.md**: Use `origin/<branch>` + `agent-<N>-work` pattern

### Follow-up PRs (P2 issues)

- Fix test assertions (IPv6, FIFO, latency bounds)
- Remove real OS calls from unit tests
- Fix wall-clock timing gates
- Document path corrections
- Add auth to parallel request script

---

## Reviewers

Parallel review by 16 agents covering all 69 changed files.
