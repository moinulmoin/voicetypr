# Remote Transcription Feature Design

**Date:** 2026-01-14
**Status:** IMPLEMENTED

## Implementation Status

### Completed (2026-01-14)
- [x] Backend: HTTP Server module (`src-tauri/src/remote/http.rs`, `lifecycle.rs`, `server.rs`)
- [x] Backend: HTTP Client module (`src-tauri/src/remote/client.rs`)
- [x] Backend: Settings storage (`src-tauri/src/remote/settings.rs`)
- [x] Backend: Tauri commands (`src-tauri/src/commands/remote.rs`)
- [x] Backend: Comprehensive logging for server/client operations
- [x] Frontend: Network Sharing card (`src/components/sections/NetworkSharingCard.tsx`)
- [x] Frontend: Models section with remote servers (`src/components/sections/ModelsSection.tsx`)
- [x] Frontend: Connection modal (`src/components/AddServerModal.tsx`)
- [x] Testing: Level 3 integration tests (`src-tauri/src/remote/integration_tests.rs`)
- [x] Beads tracking for cross-environment collaboration

### Remaining (Manual Testing)
- [ ] End-to-end verification on PC (voicetypr-hca)
- [ ] Setup MacBook as client (voicetypr-5qq)
- [ ] Concurrent transcription test (voicetypr-0k3)
- [ ] Rapid sequential requests test (voicetypr-9fj)

---

## Overview

This feature allows VoiceTypr instances to offload transcription to more powerful machines on the network. A high-end desktop with a GPU can serve as a "transcription server" for laptops and less powerful devices.

### Problem Statement

- High-end PC with RTX 3090: Near-instant transcription
- ARM MacBook: Good performance, but not instant
- Intel Mac: CPU-only mode, slow - only usable with smaller models

**Solution:** Allow slower machines to send audio to faster machines for transcription.

## Architecture

```
┌─────────────────┐         HTTP POST          ┌─────────────────┐
│  Client Device  │  ───────────────────────►  │  Server Device  │
│  (Intel Mac)    │        audio file          │  (RTX 3090 PC)  │
│                 │  ◄───────────────────────  │                 │
│                 │     transcription JSON     │                 │
└─────────────────┘                            └─────────────────┘
```

### Two Modes Per Instance

1. **Server Mode** - Share your currently selected model with other VoiceTypr instances
2. **Client Mode** - Connect to remote servers and use their models for transcription

A single machine can run both modes simultaneously.

### Key Design Decisions

- **Manual connections only** (no auto-discovery in v1)
- **Simple password authentication** (optional, user-friendly)
- **HTTP REST protocol** for simplicity
- **Single model per server** - only the currently selected model is shared
- **No automatic fallback** - if remote unreachable, show error, user manually selects different model
- **No request rejection** - server accepts all requests, processes sequentially, slower response if queued

## User Interface

### Server Mode: Settings → Network Sharing

New collapsible card in Settings section (after "Startup"):

```
┌─────────────────────────────────────────────────────────┐
│ Network Sharing                                    [▼]  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│ Share your models with other VoiceTypr instances        │
│ on your network.                                        │
│                                                         │
│ Enable Sharing                              [  Toggle  ]│
│                                                         │
│ ─ ─ ─ (shown when enabled) ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ │
│                                                         │
│ Your Address     192.168.1.50              [Copy 📋]    │
│ Port             [ 47842 ]                              │
│ Password         [ ●●●●●●●● ]  (optional)               │
│                                                         │
│ Status: Sharing large-v3-turbo • 0 active connections   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Behavior:**
- When toggle is OFF: only toggle and description visible
- When toggle is ON: IP/port/password fields appear
- IP address is auto-detected, display-only (with copy button)
- Port defaults to 47842, user can change
- Password is optional - blank means no authentication required
- All downloaded models are automatically shared when enabled
- Status shows currently selected model and connection count

### Client Mode: Models Section

**"Available to Set Up" section adds:**
```
├── large-v3 (1.5 GB)                          [Download]
├── Soniox Cloud                               [Add API Key]
└── Remote VoiceTypr Instance                  [Connect]
```

**Clicking [Connect] opens modal:**
```
┌─────────────────────────────────────────────────────────┐
│ Connect to Remote VoiceTypr                        [X]  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│ Host             [ 192.168.1.50 ]                       │
│ Port             [ 47842 ]                              │
│ Password         [ ●●●●●●●● ]  (if required)            │
│                                                         │
│              [Test Connection]                          │
│                                                         │
│ ─ ─ ─ (after successful test) ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─  │
│                                                         │
│ ✓ Connected to Desktop-PC                               │
│   Currently serving: large-v3-turbo                     │
│                                                         │
│                        [Cancel]  [Add Server]           │
└─────────────────────────────────────────────────────────┘
```

**"Available to Use" section shows remote models alongside local:**
```
Available to Use
├── base.en                                    [Select]
├── large-v3-turbo ✓                           [Selected]
├── small.en                                   [Select]
└── Desktop-PC: large-v3-turbo 🟢              [Select]
```

- Remote models appear as regular models with source indicator
- Green/red dot shows online/offline status
- Each connected server = one model (the model currently selected on that server)
- If server changes their model, the label updates on next health check

**Managing connections:**
- Each remote server has a ⋮ menu or gear icon
- Options: Edit connection, Remove, Test connection

## HTTP API Design

All endpoints prefixed with `/api/v1`. Default port: 47842.

### 1. Health Check / Status

```
GET /api/v1/status
Headers: X-VoiceTypr-Key: <password> (if required)

Response 200:
{
  "status": "ok",
  "version": "1.11.2",
  "model": "large-v3-turbo",
  "name": "Desktop-PC"
}

Response 401:
{ "error": "unauthorized" }
```

### 2. Transcribe Audio

```
POST /api/v1/transcribe
Headers:
  X-VoiceTypr-Key: <password> (if required)
  Content-Type: audio/wav
Body: <audio file bytes>

Response 200:
{
  "text": "This is the transcribed text...",
  "duration_ms": 3500,
  "model": "large-v3-turbo"
}

Response 401:
{ "error": "unauthorized" }
```

### Concurrency Handling

- Server accepts ALL transcription requests (no 503 rejection)
- Uses mutex/lock around transcription code
- Requests queue up naturally waiting for the lock
- Client sees longer response time if there's a queue
- From client perspective: send audio, eventually get text back

**Client timeout:** Different for live recordings vs uploads

*Live recordings (hotkey triggered):*
- Minimum: 30 seconds
- Maximum: 2 minutes
- Formula: `min(max(30, audio_duration_seconds), 120)`
- Rationale: Live recordings are short, transcription is faster than real-time

*Uploaded files (Upload section):*
- Based on file duration: `audio_duration_seconds + 60` (audio length + 1 minute buffer)
- No hard cap - a 4-hour file might take 5+ minutes even on a fast GPU
- Rationale: Uploaded files can be hours long, need proportional timeout

## Error Handling

### Client-Side Errors

| Scenario | When Detected | User Feedback |
|----------|---------------|---------------|
| Server unreachable | When starting recording | Toast: "Cannot reach Desktop-PC - select different model" |
| Wrong password | On connect / recording start | Toast: "Authentication failed for Desktop-PC" |
| Network timeout | During transcription | Toast: "Connection to Desktop-PC timed out" |

### Upload Feature with Remote Model

The Upload section allows users to upload audio/video files for transcription. When a remote model is selected, uploads should also use the remote server.

**Flow:**
1. User uploads audio/video file
2. If video: Extract audio locally using FFmpeg (TBD: or send video to server?)
3. If remote model selected: Send extracted audio to remote server
4. Remote server transcribes and returns text
5. Display result in Upload section

**Considerations:**
- Large files (hours of audio) may take several minutes to transcribe
- Timeout scales with file duration (no 2-minute cap for uploads)
- Progress indication needed for long uploads/transcriptions
- May want to investigate: should FFmpeg extraction happen locally or remotely?

### Recording Flow with Remote Model

1. User presses hotkey to start recording
2. Recording starts immediately
3. Network check happens in parallel
4. If remote unreachable: Show warning toast ASAP so user can stop and switch models
5. If remote reachable: Continue normally
6. After recording stops: Send audio to remote server (existing pill indicator shows transcription in progress)
7. On success: Display transcribed text
8. On failure: Show error, audio is lost (future feature: save recordings for retry)

### Model Changes

- If server changes model mid-transcription: no error, just transcribe with new model
- Response indicates which model was actually used
- Health checks update the UI label periodically - purely informational
- No need to throw errors about model mismatches

## Implementation Approach

### Backend (Rust/Tauri)

1. **New HTTP server module**
   - Lightweight HTTP server (using `axum` or `warp`)
   - Runs when sharing is enabled
   - Binds to configured port
   - Handles `/api/v1/status` and `/api/v1/transcribe`
   - Uses existing Whisper transcription code
   - Mutex around transcription for sequential processing

2. **New HTTP client module**
   - Health check function (for status polling)
   - Transcribe function (POST audio, get text back)

3. **Settings storage**
   - Store remote server connections in existing settings store
   - Structure: `{ host, port, password, friendly_name }`

### Frontend (React)

1. **Settings → Network Sharing card**
   - New collapsible section
   - Toggle, IP display, port input, password input
   - Status display

2. **Models section changes**
   - Add "Remote VoiceTypr Instance" to "Available to Set Up"
   - Show remote models in "Available to Use" alongside local models
   - Online/offline status indicators

3. **Connection modal**
   - Form for host/port/password
   - Test connection button
   - Shows discovered model name on success

### New Tauri Commands

```rust
// Server mode
start_sharing() -> Result<(), String>
stop_sharing() -> Result<(), String>
get_sharing_status() -> SharingStatus

// Client mode
add_remote_server(host, port, password) -> Result<ServerInfo, String>
remove_remote_server(server_id) -> Result<(), String>
list_remote_servers() -> Vec<RemoteServer>
test_remote_server(server_id) -> Result<ServerStatus, String>

// Transcription
transcribe_remote(server_id, audio_path) -> Result<TranscriptionResult, String>
```

## Future Enhancements (Out of Scope for v1)

1. **mDNS/Bonjour auto-discovery** - Automatically find VoiceTypr instances on local network
2. **Save recordings toggle** - Keep audio files for retry/re-transcription
3. **Re-transcribe from history** - Select past recording and transcribe with different model
4. **Multiple models per server** - Load/serve multiple models (requires memory management)
5. **HTTPS support** - Encrypted connections for internet-exposed servers
6. **Queue status** - Show position in queue and estimated wait time

## Open Questions

1. ~~**Client timeout**~~ - RESOLVED: Different for live vs uploads (see Concurrency Handling section)
2. **Health check frequency** - How often to poll server status? 30 seconds? Only when Models section open?
3. **Friendly name** - Should servers have configurable display names, or just use hostname/IP?
4. **Upload FFmpeg processing** - Should video→audio extraction happen locally before sending, or should we send raw video to server? Local extraction means less data to transfer but requires FFmpeg on client.

---

*Generated with Claude Code*
