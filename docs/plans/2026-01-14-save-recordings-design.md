# Save Recordings Feature Design

**Date:** 2026-01-14
**Status:** IMPLEMENTED
**Issue:** voicetypr-5l4

## Implementation Status

### Completed (2026-01-16)
- [x] Backend: Settings storage (`save_recordings`, `recording_retention_count`)
- [x] Backend: Recording persistence with WAV files
- [x] Backend: Count-based auto-cleanup
- [x] Backend: History extension with `recording_file` field
- [x] Frontend: Combined dropdown UI (simpler than original toggle + dropdown design)
- [x] Frontend: Re-transcribe popout with model selection
- [x] Frontend: Audio playback for saved recordings
- [x] Frontend: "Open recordings folder" link (shown inline when enabled)

---

## Overview

Allow users to save audio recordings for re-transcription with different models. Recordings are stored locally on the client, with automatic cleanup based on a configurable retention limit.

## Key Design Decisions

- **Local storage only** - Recordings saved on client, even when using remote transcription
- **Count-based cleanup** - Keep last N recordings, auto-delete oldest when exceeded
- **Linked to history** - Each transcription history item may have an associated recording file
- **No file duplication** - Re-transcriptions reference the original recording

## Settings UI (As Implemented)

### In Recording Section

**Save Recordings** (single dropdown - simplified from original toggle + dropdown design)
- Label: "Save Recordings"
- Description: "Keep audio files instead of deleting after transcription"
- Type: Dropdown combining enable/disable with retention options
- Options:
  - "Don't Save" (default - recordings disabled)
  - "Last 25"
  - "Last 50"
  - "Last 100"
  - "Last 250"
  - "Unlimited"

**Open Recordings Folder** (shown inline when save recordings is enabled)
- Small link with folder icon
- Opens the recordings directory in the system file manager

## History UI Changes

### Recording Indicator

Each history item with an associated recording file shows a small icon/button.

### Re-transcribe Menu

Clicking the icon shows a popout menu:
- Header: "Re-transcribe using..."
- Lists all available transcription sources:
  - Local models (downloaded Whisper models)
  - Remote models (connected VoiceTypr instances that are online)
  - API sources (e.g., Soniox if configured)
- Only shows sources that are currently available/online

### Re-transcription Flow

1. User selects a model from the popout
2. Creates a NEW history item immediately with "In progress..." state
3. New item references the SAME recording file (no duplication)
4. Progress indicator shown during transcription
5. On completion, text appears in the new history item
6. On failure, show error state with retry option

## Data Model

### Settings Storage

```rust
// In Settings struct
pub save_recordings: bool,           // Default: false
pub recording_retention_count: u32,  // Default: 50, 0 = unlimited
```

### Recording File Storage

- Location: `{app_data}/recordings/`
- Naming: `{timestamp}_{uuid}.wav` (e.g., `2026-01-14_143052_abc123.wav`)
- Format: WAV (same as current temp recordings)

### History Item Extension

```rust
// Add to transcription history item
pub recording_file: Option<String>,  // Filename (not full path)
pub source_recording_id: Option<String>,  // For re-transcriptions, points to original
```

## Cleanup Logic

Runs after each successful recording save:

1. List all `.wav` files in recordings directory
2. Sort by creation time (oldest first)
3. If count > retention limit:
   - Delete oldest files until count <= limit
   - Update any history items that referenced deleted files (set `recording_file` to None)

## Implementation Tasks

1. ~~**Backend: Settings** - Add `save_recordings` and `recording_retention_count` fields~~ ✅
2. ~~**Backend: Recording persistence** - Save WAV files when `save_recordings` enabled~~ ✅
3. ~~**Backend: Cleanup logic** - Implement count-based auto-cleanup~~ ✅
4. ~~**Backend: History extension** - Add `recording_file` field to history items~~ ✅
5. ~~**Frontend: Settings UI** - Combined dropdown (simplified from toggle + dropdown)~~ ✅
6. ~~**Frontend: History UI** - Recording icon, audio playback, and re-transcribe popout~~ ✅
7. ~~**Frontend: Open recordings folder** - Inline link when recordings enabled~~ ✅

## Future Considerations (Out of Scope)

- Size-based cleanup (alternative to count-based)
- Recording quality settings
- Export recordings feature
- Batch re-transcription
