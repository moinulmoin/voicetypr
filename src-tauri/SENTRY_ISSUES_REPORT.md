# Sentry Error Tracking Implementation Issues Report

## Executive Summary

Found several critical issues with Sentry error tracking implementations across 6 files. The most critical issue is incorrect timing of `configure_scope` calls, which means error context is not being attached to captured messages.

## Critical Issues Found

### 1. Timing Issue with `configure_scope` (HIGH PRIORITY)

**Problem**: `configure_scope` is called AFTER `capture_message` in all implementations, meaning tags and context are NOT attached to the errors being sent to Sentry.

**Affected Files**:
- `commands/audio.rs` - 6 occurrences
- `whisper/transcriber.rs` - 4 occurrences  
- `commands/permissions.rs` - 3 occurrences
- `commands/model.rs` - 1 occurrence
- `whisper/manager.rs` - 2 occurrences
- `commands/text.rs` - 6 occurrences

**Example of Current (WRONG) Implementation**:
```rust
// WRONG - configure_scope happens after capture
sentry::capture_message(
    &format!("Audio recording failed: {}", e),
    sentry::Level::Error,
);
sentry::configure_scope(|scope| {
    scope.set_tag("error.type", error_type);
    scope.set_tag("component", "audio_recorder");
});
```

**Correct Implementation**:
```rust
// CORRECT - configure scope BEFORE capture
sentry::with_scope(|scope| {
    scope.set_tag("error.type", error_type);
    scope.set_tag("component", "audio_recorder");
    sentry::capture_message(
        &format!("Audio recording failed: {}", e),
        sentry::Level::Error,
    );
}, || {});
```

### 2. Privacy/Security Concerns (MEDIUM PRIORITY)

**Issues**:
- Full file paths being sent (could reveal directory structure)
- Text lengths being sent (might be sensitive)
- No PII filtering evident

**Recommendations**:
- Use the `sanitize_path` helper from `utils/sentry_helper.rs`
- Avoid sending full paths or sensitive data
- Configure Sentry SDK with PII filters

### 3. Context Conflicts in Async Code (LOW PRIORITY)

**Problem**: Multiple async operations might overwrite each other's scope context.

**Solution**: Use `with_scope` instead of `configure_scope` to create isolated scopes.

### 4. No Issues Found With:
- ✅ Error Propagation - Errors are correctly returned after Sentry capture
- ✅ Duplicate Captures - No duplicate error captures detected
- ✅ Performance Impact - Sentry calls are in error paths only

## Recommended Actions

### Immediate Actions (Do First):

1. **Fix all timing issues** - Use the helper macros from `utils/sentry_helper.rs`:
   ```rust
   use crate::capture_sentry_message;
   
   capture_sentry_message!(
       &format!("Audio recording failed: {}", e),
       sentry::Level::Error,
       "error.type" => error_type,
       "component" => "audio_recorder",
       "operation" => "start_recording"
   );
   ```

2. **Sanitize file paths** before sending:
   ```rust
   use crate::utils::sentry_helper::sanitize_path;
   
   let safe_filename = sanitize_path(&audio_path);
   ```

### Example Fix for audio.rs

Here's how to fix the Sentry implementation in `commands/audio.rs`:

**Line 227-236 (start_recording error)**:
```rust
// OLD (WRONG):
sentry::capture_message(
    &format!("Audio recording failed: {}", e),
    sentry::Level::Error,
);
sentry::configure_scope(|scope| {
    scope.set_tag("error.type", error_type);
    scope.set_tag("component", "audio_recorder");
    scope.set_tag("operation", "start_recording");
});

// NEW (CORRECT):
use crate::capture_sentry_message;

capture_sentry_message!(
    &format!("Audio recording failed: {}", e),
    sentry::Level::Error,
    "error.type" => error_type,
    "component" => "audio_recorder",
    "operation" => "start_recording"
);
```

**Line 394-406 (stop_recording error)**:
```rust
// OLD (WRONG):
sentry::capture_message(
    &format!("Failed to stop recording: {}", e),
    sentry::Level::Error,
);
sentry::configure_scope(|scope| {
    scope.set_tag("error.type", "recording_stop_failure");
    scope.set_tag("component", "audio_recorder");
    scope.set_tag("operation", "stop_recording");
});

// NEW (CORRECT):
capture_sentry_message!(
    &format!("Failed to stop recording: {}", e),
    sentry::Level::Error,
    "error.type" => "recording_stop_failure",
    "component" => "audio_recorder",
    "operation" => "stop_recording"
);
```

### Testing Recommendations

1. Test that Sentry events include proper tags/context after fixes
2. Verify no sensitive data is being sent
3. Test concurrent error scenarios to ensure no context conflicts
4. Monitor Sentry dashboard to confirm proper error grouping

## Files Requiring Updates

Priority order for fixing:
1. `commands/audio.rs` - Core functionality, 6 issues
2. `commands/text.rs` - User-facing feature, 6 issues  
3. `whisper/transcriber.rs` - Critical path, 4 issues
4. `commands/permissions.rs` - Setup flow, 3 issues
5. `whisper/manager.rs` - Model management, 2 issues
6. `commands/model.rs` - Model downloads, 1 issue

## Next Steps

1. Import the helper macros in each affected file
2. Replace all `configure_scope` + `capture_message` with the helper macros
3. Sanitize any file paths before sending to Sentry
4. Test the changes to ensure proper error tracking
5. Consider adding Sentry transaction support for complex flows