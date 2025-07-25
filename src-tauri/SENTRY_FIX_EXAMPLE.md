# Example Sentry Fixes for audio.rs

## Step 1: Add imports at the top of the file

```rust
use crate::capture_sentry_message;
use crate::capture_sentry_with_context;
use crate::utils::sentry_helper::{sanitize_path, create_safe_file_context};
```

## Step 2: Fix each Sentry capture

### Fix 1: Line 194-199 (Recording failed to start)
```rust
// REMOVE these lines:
use tauri_plugin_sentry::sentry;
sentry::capture_message(
    "Recording failed to start after successful initialization",
    sentry::Level::Error,
);

// REPLACE with:
capture_sentry_message!(
    "Recording failed to start after successful initialization",
    sentry::Level::Error,
    "error.type" => "recording_init_failure",
    "component" => "audio_recorder",
    "operation" => "start_recording"
);
```

### Fix 2: Line 227-236 (Audio recording failed)
```rust
// REMOVE:
sentry::capture_message(
    &format!("Audio recording failed: {}", e),
    sentry::Level::Error,
);
sentry::configure_scope(|scope| {
    scope.set_tag("error.type", error_type);
    scope.set_tag("component", "audio_recorder");
    scope.set_tag("operation", "start_recording");
});

// REPLACE with:
capture_sentry_message!(
    &format!("Audio recording failed: {}", e),
    sentry::Level::Error,
    "error.type" => error_type,
    "component" => "audio_recorder",
    "operation" => "start_recording"
);
```

### Fix 3: Line 394-406 (Stop recording failure)
```rust
// REMOVE:
use tauri_plugin_sentry::sentry;
sentry::capture_message(
    &format!("Failed to stop recording: {}", e),
    sentry::Level::Error,
);
sentry::configure_scope(|scope| {
    scope.set_tag("error.type", "recording_stop_failure");
    scope.set_tag("component", "audio_recorder");
    scope.set_tag("operation", "stop_recording");
});

// REPLACE with:
capture_sentry_message!(
    &format!("Failed to stop recording: {}", e),
    sentry::Level::Error,
    "error.type" => "recording_stop_failure",
    "component" => "audio_recorder",
    "operation" => "stop_recording"
);
```

## Step 3: Fix file path privacy issues

For any place where file paths are being used, sanitize them:

```rust
// Instead of sending full path:
let audio_path = recordings_dir.join(format!("recording_{}.wav", timestamp));

// Send sanitized version to Sentry:
let safe_filename = sanitize_path(&audio_path);
// Use safe_filename in any Sentry contexts
```

## Complete Example with Context

If you need to send context with file information:

```rust
capture_sentry_with_context!(
    &format!("Failed to open audio file: {}", e),
    sentry::Level::Error,
    tags: {
        "error.type" => "file_access_error",
        "component" => "audio_recorder"
    },
    context: {
        "file" => {
            "filename" => sanitize_path(&audio_path),
            "exists" => audio_path.exists(),
            "operation" => "transcription"
        }
    }
);
```

## Testing the Fix

After making these changes:

1. Run the test_sentry command to verify errors are captured with proper context
2. Check Sentry dashboard to ensure tags are attached
3. Verify no sensitive paths are exposed

## Benefits of This Approach

1. **Correct timing** - Tags/context are set before capture
2. **Thread-safe** - Each capture has its own scope
3. **Privacy-preserving** - File paths are sanitized
4. **Consistent** - Same pattern everywhere
5. **Maintainable** - Helper macros reduce boilerplate