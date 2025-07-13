# Transcription Debugging Guide

## Summary of Changes

I've added extensive debug logging throughout the transcription flow to help identify where the issue is occurring. The symptoms suggest the transcription is failing somewhere between audio recording and the pill window receiving the event.

## Debug Logging Added

### 1. **Transcriber Module** (`src-tauri/src/whisper/transcriber.rs`)
- Logs when transcription starts with file path
- Checks if audio file exists and logs its size
- Logs WAV file specifications
- Logs each step of Whisper processing
- Logs the final transcription result or any errors

### 2. **Transcriber Cache** (`src-tauri/src/whisper/cache.rs`)
- Logs model loading attempts
- Checks if model file exists before loading
- Logs cache hits/misses
- Logs model loading time

### 3. **Audio Commands** (`src-tauri/src/commands/audio.rs`)
- Logs transcribed text content and length
- Logs success/failure of event emission

### 4. **Window Manager** (`src-tauri/src/window_manager.rs`)
- Logs all event emission attempts
- Logs window visibility state
- Logs payload for transcription-complete events
- Falls back to app-wide emission if window-specific fails

### 5. **React Components**
- Added debug logging to RecordingPill component
- Logs when component mounts/unmounts
- Logs when event handlers are registered
- Logs received events with full details

## Debug Commands Added

Two new Tauri commands for testing:

1. **`debug_transcription_flow`** - Checks:
   - Window manager initialization
   - Pill window existence and visibility
   - Tests event emission
   - Shows current recording state

2. **`test_transcription_event`** - Sends a test transcription-complete event

## How to Debug

1. **Enable Debug Logging**:
   ```bash
   RUST_LOG=debug pnpm tauri dev
   ```

2. **Watch the Console** for lines containing `[TRANSCRIPTION_DEBUG]`

3. **Expected Flow**:
   - Audio file created and saved
   - Model loaded from cache or disk
   - Whisper processes the audio
   - Text is extracted from segments
   - Event is emitted to pill window
   - Pill window receives event and processes it

4. **Common Issues to Look For**:
   - "Audio file does not exist" - Recording failed to save
   - "Model file does not exist" - Model path is wrong
   - "Failed to open WAV file" - Audio file is corrupted
   - "Whisper inference failed" - Model or audio issue
   - "pill window not found" - Window wasn't created
   - "transcription-complete event received" not appearing - Event not reaching React

## Testing the Debug Panel

Add this to your App.tsx to see the debug panel:

```tsx
import { DebugPanel } from "@/components/DebugPanel";

// In your App component JSX:
<DebugPanel />
```

## Key Areas to Check

1. **Model Loading**: Is the correct model being loaded? Check the path.
2. **Audio File**: Is the WAV file being created with proper size?
3. **Whisper Processing**: Are segments being generated?
4. **Event Flow**: Is the event being emitted and received?
5. **Pill Window**: Is it created and visible when events are sent?

## Next Steps

Run the app with debug logging and perform a recording. Look for:
1. Where in the flow does it stop?
2. Are there any error messages?
3. Is the transcription completing but the event not being sent?
4. Is the event being sent but not received?

The debug output will help pinpoint the exact failure point.