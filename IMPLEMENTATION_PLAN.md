# Implementation Plan: ESC Handler Refactor & Toast Testing

**Date:** 2025-12-11  
**Status:** Ready for execution  
**Priority:** Medium (non-blocking improvements)

---

## Overview

Execute three high-impact improvements identified in oracle review:
1. Extract ESC handler to dedicated module (readability)
2. Add comprehensive toast lifecycle tests (reliability)
3. Document toast positioning constraints (maintainability)

---

## Phase 1: Extract ESC Handler Module

**Goal:** Reduce lib.rs complexity, improve testability

### Task 1.1: Create `src-tauri/src/recording/escape_handler.rs`

**Current State:**
- ESC logic scattered in `lib.rs:1186-1280` (~95 lines)
- Handles: double-tap detection, timeout, recording cancellation
- Uses: `AppState`, `app_handle`, atomic counters

**Implementation:**
```rust
// src-tauri/src/recording/escape_handler.rs

use tauri::AppHandle;
use crate::AppState;
use tauri_plugin_global_shortcut::ShortcutState;
use std::sync::atomic::Ordering;

pub async fn handle_escape_key(
    app_state: &AppState,
    app_handle: &AppHandle,
    event_state: ShortcutState,
) -> Result<(), String> {
    // Only react to key press (not release)
    if event_state != ShortcutState::Pressed {
        log::debug!("Ignoring ESC event (not a press): {:?}", event_state);
        return Ok(());
    }

    let current_state = get_recording_state(app_handle);
    
    // Only handle during active recording
    if !matches!(current_state, RecordingState::Recording | RecordingState::Transcribing | ...) {
        return Ok(());
    }

    let was_pressed_once = app_state.esc_pressed_once.load(Ordering::SeqCst);
    
    if !was_pressed_once {
        handle_first_esc_press(app_state, app_handle).await
    } else {
        handle_second_esc_press(app_state, app_handle).await
    }
}

async fn handle_first_esc_press(
    app_state: &AppState,
    app_handle: &AppHandle,
) -> Result<(), String> {
    log::info!("First ESC press detected");
    app_state.esc_pressed_once.store(true, Ordering::SeqCst);

    crate::commands::audio::pill_toast(
        app_handle,
        "Press ESC again to cancel",
        1200,
    );

    // Set timeout to reset ESC state
    let app_for_timeout = app_handle.clone();
    let timeout_handle = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let app_state = app_for_timeout.state::<AppState>();
        app_state.esc_pressed_once.store(false, Ordering::SeqCst);
        log::debug!("ESC timeout expired, resetting state");
    });

    // Store timeout handle
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(old_handle) = timeout_guard.take() {
            old_handle.abort();
        }
        *timeout_guard = Some(timeout_handle);
    }

    Ok(())
}

async fn handle_second_esc_press(
    app_state: &AppState,
    app_handle: &AppHandle,
) -> Result<(), String> {
    log::info!("Second ESC press detected, cancelling recording");

    // Hide toast immediately
    if let Some(toast_window) = app_handle.get_webview_window("toast") {
        let _ = toast_window.hide();
    }

    // Cancel timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    // Reset state
    app_state.esc_pressed_once.store(false, Ordering::SeqCst);

    // Cancel recording
    let app_for_cancel = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = cancel_recording(app_for_cancel).await {
            log::error!("Failed to cancel recording: {}", e);
        }
    });

    Ok(())
}
```

**Effort:** 2 hours  
**Risk:** Low (pure refactor, same logic)

---

### Task 1.2: Update `src-tauri/src/recording/mod.rs`

**Add module declaration:**
```rust
pub mod escape_handler;

pub use escape_handler::handle_escape_key;
```

**Effort:** 30 min

---

### Task 1.3: Update `src-tauri/src/lib.rs`

**Replace lines 1186-1280 with:**
```rust
if shortcut == &escape_shortcut {
    if let Err(e) = crate::recording::handle_escape_key(
        &app_state,
        &app_handle,
        event.state(),
    ).await {
        log::error!("ESC handler error: {}", e);
    }
    return;
}
```

**Effort:** 30 min  
**Benefit:** lib.rs reduced by ~95 lines

---

## Phase 2: Add Toast Lifecycle Tests

**Goal:** Verify toast behavior under stress conditions

### Task 2.1: Create `src-tauri/src/commands/audio.rs::tests`

**Test: Concurrent Toast Messages**
```rust
#[tokio::test]
async fn test_concurrent_toasts() {
    // Simulate 10 rapid toast messages
    // Verify only the latest one is visible
    // Check ID counter prevents race conditions
}

#[tokio::test]
async fn test_toast_duration_respected() {
    // Show toast with 500ms duration
    // Verify auto-hide after 500ms
    // Verify manual hide works before timeout
}

#[tokio::test]
async fn test_toast_window_not_found() {
    // Simulate missing toast window
    // Verify pill_toast doesn't panic
    // Check warning logged
}

#[tokio::test]
async fn test_esc_handler_shows_toast() {
    // Trigger first ESC press during recording
    // Verify pill_toast called with correct message
    // Verify timer set to 1200ms
}

#[tokio::test]
async fn test_esc_state_timeout_reset() {
    // Trigger first ESC
    // Wait 3+ seconds
    // Verify esc_pressed_once resets to false
}
```

**Effort:** 3 hours  
**Benefit:** 90%+ confidence in toast behavior

---

### Task 2.2: Create Integration Test `src-tauri/src/tests/toast_integration.rs`

```rust
#[tokio::test]
async fn test_full_esc_cancel_flow() {
    // 1. Start recording
    // 2. Press ESC once → verify "Press ESC again to cancel" shown
    // 3. Wait for timeout → verify message hidden
    // 4. Press ESC again → verify recording cancelled
    // 5. Verify toast window hidden
}
```

**Effort:** 2 hours

---

## Phase 3: Document Toast Positioning

**Goal:** Prevent future regressions in pill/toast layout

### Task 3.1: Update `src-tauri/src/lib.rs` comments

**Current code (lines 1936-1944):**
```rust
// Create toast window for feedback messages (positioned above pill) - all platforms
let toast_width = 280.0;
let toast_height = 32.0;
let pill_width = 80.0;
let gap = 6.0; // Gap between pill and toast

// Center toast above pill
let toast_x = pos_x + (pill_width - toast_width) / 2.0;
let toast_y = pos_y - toast_height - gap;
```

**Add documentation:**
```rust
/// TOAST WINDOW POSITIONING
///
/// Toast is positioned above pill with these constraints:
/// - Width: 280px (accommodates "Press ESC again to cancel")
/// - Height: 32px (text + padding)
/// - Gap: 6px above pill
///
/// IMPORTANT: If pill position changes, update these calculations:
/// - Pill is centered at screen bottom
/// - Toast must be centered above pill: x = pill_x + (pill_width - toast_width) / 2
/// - Ensure toast doesn't go off-screen on small displays
///
/// See: window_manager.rs::show_pill_window_internal() for pill positioning
```

**Effort:** 1 hour

---

### Task 3.2: Create `TOAST_POSITIONING.md`

**Content:**
```markdown
# Toast Window Positioning Rules

## Design
- Toast appears above pill window
- Centered horizontally relative to pill
- 6px vertical gap between windows

## Calculations
```
toast_x = pill_center_x - (toast_width / 2)
toast_y = pill_top_y - toast_height - gap
```

## Constraints
- Toast must stay on-screen on all display sizes
- Minimum screen height: 440px (toast_height + gap + pill_height + margins)
- Toast width (280px) must fit within screen width

## Testing
- Test on 1024x768 (smallest supported)
- Test on 4K displays
- Test fullscreen vs windowed
```

**Effort:** 1 hour

---

## Summary Table

| Phase | Task | Effort | Risk | Blocker |
|-------|------|--------|------|---------|
| 1.1 | Extract escape_handler.rs | 2h | Low | No |
| 1.2 | Add module declaration | 30m | Minimal | No |
| 1.3 | Update lib.rs | 30m | Low | No |
| 2.1 | Add unit tests | 3h | Minimal | No |
| 2.2 | Add integration tests | 2h | Minimal | No |
| 3.1 | Document positioning | 1h | None | No |
| 3.2 | Create TOAST_POSITIONING.md | 1h | None | No |
| **TOTAL** | | **10.5h** | **Low** | **None** |

---

## Execution Order

**Week 1 (Priority):**
1. Phase 1: Extract ESC handler (3 hours)
   - Cleaner code, easier to test
   
2. Phase 2.1: Add unit tests (3 hours)
   - Catch regressions early

**Week 2 (Follow-up):**
3. Phase 2.2: Integration tests (2 hours)
   - End-to-end validation

4. Phase 3: Document positioning (2 hours)
   - Knowledge transfer, prevent future bugs

---

## Success Criteria

✓ All tests passing  
✓ lib.rs reduced by ~95 lines (readability)  
✓ ESC handler fully tested  
✓ Toast lifecycle documented  
✓ No regressions in existing functionality  
✓ Code review approval from team  

---

## Rollback Plan

Each phase is independent:
- If Phase 1 fails: revert escape_handler.rs, keep original lib.rs
- If Phase 2 fails: remove new tests, keep working code
- If Phase 3 fails: documentation only, no code changes

---

## Notes

- **Capabilities fix (this PR):** Required, merge immediately
- **Refactoring (this plan):** Nice-to-have improvements, can merge incrementally
- **Testing focus:** Toast behavior under stress, ESC handler state machine
- **Documentation:** Positioning constraints + module organization

