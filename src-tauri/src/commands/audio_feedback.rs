//! Best-effort, non-blocking audio feedback for user-visible lifecycle events.

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AudioFeedbackCue {
    RecordingStarted,
    TranscriptReady,
    PasteCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CueSpec {
    setting_key: &'static str,
    macos_sound_path: &'static str,
    windows_frequency_hz: u32,
    windows_duration_ms: u32,
}

const fn cue_spec(cue: AudioFeedbackCue) -> CueSpec {
    match cue {
        AudioFeedbackCue::RecordingStarted => CueSpec {
            setting_key: "play_sound_on_recording",
            macos_sound_path: "/System/Library/Sounds/Tink.aiff",
            windows_frequency_hz: 800,
            windows_duration_ms: 100,
        },
        AudioFeedbackCue::TranscriptReady => CueSpec {
            setting_key: "play_sound_on_transcription_complete",
            macos_sound_path: "/System/Library/Sounds/Pop.aiff",
            windows_frequency_hz: 600,
            windows_duration_ms: 100,
        },
        AudioFeedbackCue::PasteCompleted => CueSpec {
            setting_key: "play_sound_on_paste_success",
            macos_sound_path: "/System/Library/Sounds/Glass.aiff",
            windows_frequency_hz: 1_000,
            windows_duration_ms: 100,
        },
    }
}

fn cue_enabled(
    cue: AudioFeedbackCue,
    stored: Option<bool>,
    legacy_recording_end: Option<bool>,
) -> bool {
    match cue {
        AudioFeedbackCue::TranscriptReady => {
            crate::commands::settings::resolve_transcription_complete_sound(
                stored,
                legacy_recording_end,
            )
        }
        AudioFeedbackCue::RecordingStarted | AudioFeedbackCue::PasteCompleted => {
            stored.unwrap_or(true)
        }
    }
}

/// Starts the platform sound process and returns immediately. Feedback is
/// best-effort: store or process-launch failures are logged and never affect
/// recording, delivery, or clipboard restoration.
pub(crate) fn play_audio_feedback(app: &AppHandle, cue: AudioFeedbackCue) {
    let spec = cue_spec(cue);
    let store = match app.store("settings") {
        Ok(store) => store,
        Err(error) => {
            log::warn!(
                "Skipping {:?} audio feedback because settings are unavailable: {}",
                cue,
                error
            );
            return;
        }
    };
    let enabled = cue_enabled(
        cue,
        store
            .get(spec.setting_key)
            .and_then(|value| value.as_bool()),
        store
            .get("play_sound_on_recording_end")
            .and_then(|value| value.as_bool()),
    );
    if !enabled {
        return;
    }

    play_platform_cue(cue, spec);
}

fn play_platform_cue(cue: AudioFeedbackCue, spec: CueSpec) {
    #[cfg(target_os = "macos")]
    if let Err(error) = std::process::Command::new("afplay")
        .arg(spec.macos_sound_path)
        .spawn()
    {
        log::warn!("Failed to play {:?} audio feedback: {}", cue, error);
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = format!(
            "[console]::beep({}, {})",
            spec.windows_frequency_hz, spec.windows_duration_ms
        );
        if let Err(error) = std::process::Command::new("powershell")
            .args(["-c", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            log::warn!("Failed to play {:?} audio feedback: {}", cue, error);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (cue, spec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn cues_have_distinct_settings_and_platform_sounds() {
        let specs = [
            cue_spec(AudioFeedbackCue::RecordingStarted),
            cue_spec(AudioFeedbackCue::TranscriptReady),
            cue_spec(AudioFeedbackCue::PasteCompleted),
        ];

        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.setting_key)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.macos_sound_path)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.windows_frequency_hz)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn transcript_ready_honors_legacy_disabled_preference() {
        assert!(!cue_enabled(
            AudioFeedbackCue::TranscriptReady,
            None,
            Some(false)
        ));
        assert!(cue_enabled(
            AudioFeedbackCue::TranscriptReady,
            Some(true),
            Some(false)
        ));
        assert!(cue_enabled(
            AudioFeedbackCue::PasteCompleted,
            None,
            Some(false)
        ));
    }
}
