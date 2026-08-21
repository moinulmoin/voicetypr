# Plan 058 — Pause & resume media during dictation

**Status:** TODO
**Priority:** P1
**Effort:** M
**Depends on:** 057 (shared recorder hooks; can start in parallel on the settings/UI side)

## Problem

User listens to music/video, hits the dictation hotkey, and their speech lands
on top of playback (or worse, music bleeds into the transcript). VoiceInk ships
this as "Pause Media While Recording" + "Resume Delay (0–5s)" — proven QoL
feature, proven implementation path (their code is public).

## Verified mechanism inventory (researched 2026-08-21)

### macOS — how VoiceInk actually does it (Beingpax/VoiceInk source, read directly)

1. **Pause**: private `MediaRemote.framework` via `ejbills/mediaremote-adapter`,
   which bridges through **the system's entitled Perl interpreter**
   (`/usr/bin/perl` dlopens MediaRemote — required since macOS 14.4 restricted
   mediaremoted XPC to entitled callers). Provides now-playing track info
   (bundleId, isPlaying, playbackRate, PID) + `pause()`.
2. **Resume**: after the configured delay, synthesize the **NX_KEYTYPE_PLAY (16)
   system-defined HID event** (VoiceInk `sendMediaPlayPauseKey()` — some apps,
   e.g. Plexamp, ignore MediaRemote `play` but obey the F8 key event).
3. **Fallback**: CoreAudio `kAudioDevicePropertyMute` on the default output —
   public API, with `was-muted-before` bookkeeping + a generation counter so
   concurrent recordings can't race the unmute.
4. **Safety bookkeeping** (steal all of it): only pause if now-playing says
   `isPlaying == true` (never blind-toggle); on resume verify the original app
   is still running, still the current now-playing app, and still paused.

### Windows — supported public API

`GlobalSystemMediaTransportControlsSessionManager` (SMTC — the API behind the
Volume Mixer / media flyout the user described):

- `windows` crate, features `["Foundation", "Media_Control"]`; or `win-gsmtc`.
- `GetSessions()` → per-app sessions with `PlaybackStatus` (Playing/Paused) and
  `SourceAppUserModelId`; `TryPauseAsync()` / `TryPlayAsync()` per session.
- Pause every session currently `Playing`, remember exactly which we paused,
  resume only those. No blind toggling possible.
- **MSIX/Store build requires the `globalMediaControl` capability** in the
  package manifest; NSIS build needs nothing.

## Scope

1. Backend module `src-tauri/src/media/`:
   - `pause_media()` / `resume_media(delay_ms)` called from the recording start
     path (audio.rs:4168-4188) and stop path (audio.rs:4783-4799 + cancel
     7469) — exact hook points already identified.
   - macOS: perl-bridge MediaRemote client (vendor the adapter's approach: small
     run.pl + JSON stdout protocol) + NX play-key synthesizer + mute fallback.
     All three behind one trait.
   - Windows: SMTC client with paused-session ledger; SendInput
     `VK_MEDIA_PLAY_PAUSE` as last-resort fallback.
2. Settings (General → Behavior): "Pause media while recording" toggle
   (default **on** — user-requested QoL) + "Resume delay" 0–5s slider +
   "Mute instead of pause" fallback toggle.
3. Bookkeeping guards (both OS): only-if-playing, app-still-running,
   same-session check, resume-task cancellation on re-record, generation
   counter.
4. Telemetry: pause attempted/succeeded/resumed events (no app names — privacy).

## Non-goals

- Per-app allowlists (VoiceInk's `SupportedMedia` list is for file import, not
  pause; SMTC/MediaRemote cover the playing app by construction).
- System-audio capture/ducking (057 notes the loopback option separately).
- Linux (not shipped).

## Acceptance

1. Gates green; focused Rust tests for the ledger state machine (fake clock).
2. Smoke (SMOKE.md when claimed):
   - macOS: Spotify + Music.app + YouTube (Safari/Chrome) pause on record,
     resume after delay; recording with nothing playing changes nothing;
     cancel path still resumes; user-paused-before stays paused.
   - Windows: Spotify + a browser video pause/resume; MSIX build manifest
     carries `globalMediaControl`.
3. No interaction regression with 059's no-speech gate (both hook the same
   start/stop path).

## STOP conditions

- If Apple breaks the perl bridge on a macOS beta we test (listener terminated),
  feature degrades to mute-fallback automatically — ship that, note it in UI
  copy, do not chase private-API alternatives.
