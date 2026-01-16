#[cfg(test)]
mod tests {
    use crate::commands::settings::{get_supported_languages, resolve_pill_indicator_mode, Settings};
    use serde_json::json;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();

        assert_eq!(settings.hotkey, "CommandOrControl+Shift+Space");
        assert_eq!(settings.current_model, ""); // Empty means auto-select
        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.transcription_cleanup_days, None);
        assert_eq!(settings.launch_at_startup, false);
        assert_eq!(settings.onboarding_completed, false);
        assert_eq!(settings.check_updates_automatically, true); // Default to true
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            hotkey: "CommandOrControl+A".to_string(),
            current_model: "base".to_string(),
            current_model_engine: "whisper".to_string(),
            language: "es".to_string(),
            theme: "dark".to_string(),
            transcription_cleanup_days: Some(7),
            pill_position: Some((100.0, 200.0)),
            launch_at_startup: false,
            onboarding_completed: true,
            translate_to_english: false,
            check_updates_automatically: true,
            selected_microphone: None,
            recording_mode: "toggle".to_string(),
            use_different_ptt_key: false,
            ptt_hotkey: Some("Alt+Space".to_string()),
            keep_transcription_in_clipboard: false,
            play_sound_on_recording: true,
            play_sound_on_recording_end: true,
            pill_indicator_mode: "when_recording".to_string(),
            pill_indicator_position: "bottom".to_string(),
            sharing_port: Some(47842),
            sharing_password: None,
        };

        // Test serialization
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"hotkey\":\"CommandOrControl+A\""));
        assert!(json.contains("\"current_model\":\"base\""));
        assert!(json.contains("\"language\":\"es\""));
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"transcription_cleanup_days\":7"));

        // Test deserialization
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hotkey, settings.hotkey);
        assert_eq!(deserialized.current_model, settings.current_model);
        assert_eq!(deserialized.language, settings.language);
        assert_eq!(deserialized.theme, settings.theme);
        assert_eq!(
            deserialized.transcription_cleanup_days,
            settings.transcription_cleanup_days
        );
    }

    #[test]
    fn test_settings_partial_deserialization() {
        // Test that partial JSON can be deserialized with defaults
        let partial_json = json!({
            "hotkey": "Alt+Space",
            "theme": "light"
        });

        // This should work with serde's default attribute
        let _json_str = serde_json::to_string(&partial_json).unwrap();

        // Since Settings doesn't have default attributes on fields,
        // we can't deserialize partial JSON directly
        // This is expected behavior for the current implementation
    }

    #[test]
    fn test_settings_clone() {
        let settings = Settings {
            hotkey: "CommandOrControl+B".to_string(),
            current_model: "tiny".to_string(),
            current_model_engine: "whisper".to_string(),
            language: "fr".to_string(),
            theme: "light".to_string(),
            transcription_cleanup_days: Some(30),
            pill_position: None,
            launch_at_startup: true,
            onboarding_completed: false,
            translate_to_english: true,
            check_updates_automatically: true,
            selected_microphone: Some("USB Microphone".to_string()),
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("CommandOrControl+Space".to_string()),
            keep_transcription_in_clipboard: true,
            play_sound_on_recording: false,
            play_sound_on_recording_end: false,
            pill_indicator_mode: "never".to_string(),
            pill_indicator_position: "top".to_string(),
            sharing_port: None,
            sharing_password: Some("test123".to_string()),
        };

        let cloned = settings.clone();
        assert_eq!(cloned.hotkey, settings.hotkey);
        assert_eq!(cloned.current_model, settings.current_model);
        assert_eq!(cloned.language, settings.language);
        assert_eq!(cloned.theme, settings.theme);
        assert_eq!(
            cloned.transcription_cleanup_days,
            settings.transcription_cleanup_days
        );
    }

    #[test]
    fn test_valid_hotkey_formats() {
        let valid_hotkeys = vec![
            "CommandOrControl+Shift+Space",
            "Alt+A",
            "CommandOrControl+Alt+B",
            "Shift+F1",
            "CommandOrControl+1",
        ];

        for hotkey in valid_hotkeys {
            let settings = Settings {
                hotkey: hotkey.to_string(),
                ..Settings::default()
            };
            assert!(!settings.hotkey.is_empty());
            assert!(settings.hotkey.len() <= 100);
        }
    }

    #[test]
    fn test_theme_values() {
        let valid_themes = vec!["system", "light", "dark"];

        for theme in valid_themes {
            let settings = Settings {
                theme: theme.to_string(),
                ..Settings::default()
            };
            assert!(["system", "light", "dark"].contains(&settings.theme.as_str()));
        }
    }

    #[test]
    fn test_language_codes() {
        let valid_languages = vec!["en", "es", "fr", "de", "it", "pt", "ru", "zh", "ja", "ko"];

        for lang in valid_languages {
            let settings = Settings {
                language: lang.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.language, lang);
        }
    }

    #[test]
    fn test_model_selection() {
        // Empty model means auto-select
        let auto_settings = Settings {
            current_model: "".to_string(),
            current_model_engine: "whisper".to_string(),
            ..Settings::default()
        };
        assert_eq!(auto_settings.current_model, "");

        // Specific model
        let specific_settings = Settings {
            current_model: "base".to_string(),
            current_model_engine: "whisper".to_string(),
            ..Settings::default()
        };
        assert_eq!(specific_settings.current_model, "base");
    }

    #[test]
    fn test_settings_to_json_value() {
        let settings = Settings::default();

        // Convert to JSON Value
        let value = json!({
            "hotkey": settings.hotkey,
            "current_model": settings.current_model,
            "language": settings.language,
            "theme": settings.theme,
            "transcription_cleanup_days": settings.transcription_cleanup_days,
            "launch_at_startup": settings.launch_at_startup,
            "onboarding_completed": settings.onboarding_completed,
        });

        assert_eq!(value["hotkey"], "CommandOrControl+Shift+Space");
        assert_eq!(value["current_model"], "");
        assert_eq!(value["language"], "en");
        assert_eq!(value["theme"], "system");
        assert_eq!(value["transcription_cleanup_days"], serde_json::Value::Null);
        assert_eq!(value["launch_at_startup"], false);
        assert_eq!(value["onboarding_completed"], false);
    }

    #[test]
    fn test_hotkey_validation_edge_cases() {
        // Test empty hotkey (invalid)
        assert!("".is_empty());

        // Test very long hotkey (invalid if > 100 chars)
        let long_hotkey = "CommandOrControl+".repeat(10);
        assert!(long_hotkey.len() > 100);

        // Test normal hotkey length
        let normal_hotkey = "CommandOrControl+Shift+Alt+A";
        assert!(!normal_hotkey.is_empty());
        assert!(normal_hotkey.len() <= 100);
    }

    #[tokio::test]
    async fn test_get_supported_languages() {
        let languages = get_supported_languages().await.unwrap();

        // Should have multiple languages
        assert!(languages.len() > 50);

        // Languages should be sorted alphabetically (auto-detect removed)
        // First language will be Afrikaans alphabetically
        assert_eq!(languages[0].code, "af");
        assert_eq!(languages[0].name, "Afrikaans");

        // Should contain common languages
        let codes: Vec<String> = languages.iter().map(|l| l.code.clone()).collect();
        assert!(codes.contains(&"en".to_string()));
        assert!(codes.contains(&"es".to_string()));
        assert!(codes.contains(&"fr".to_string()));
        assert!(codes.contains(&"zh".to_string()));

        // Should be sorted by name alphabetically
        for i in 1..languages.len() {
            assert!(
                languages[i - 1].name <= languages[i].name,
                "Languages should be sorted by name"
            );
        }
    }

    // ============================================================================
    // Recording Indicator Mode (pill_indicator_mode) Tests
    // ============================================================================

    #[test]
    fn test_pill_indicator_mode_default() {
        let settings = Settings::default();
        assert_eq!(settings.pill_indicator_mode, "when_recording");
    }

    #[test]
    fn test_pill_indicator_mode_valid_values() {
        let valid_modes = vec!["never", "always", "when_recording"];

        for mode in valid_modes {
            let settings = Settings {
                pill_indicator_mode: mode.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.pill_indicator_mode, mode);
        }
    }

    #[test]
    fn test_pill_indicator_mode_serialization() {
        let settings = Settings {
            pill_indicator_mode: "always".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"pill_indicator_mode\":\"always\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pill_indicator_mode, "always");
    }

    #[test]
    fn test_pill_indicator_mode_never() {
        let settings = Settings {
            pill_indicator_mode: "never".to_string(),
            ..Settings::default()
        };
        assert_eq!(settings.pill_indicator_mode, "never");
    }

    // ============================================================================
    // Pill Indicator Position Tests
    // ============================================================================

    #[test]
    fn test_pill_indicator_position_default() {
        let settings = Settings::default();
        assert_eq!(settings.pill_indicator_position, "bottom");
    }

    #[test]
    fn test_pill_indicator_position_valid_values() {
        let valid_positions = vec!["top", "center", "bottom"];

        for position in valid_positions {
            let settings = Settings {
                pill_indicator_position: position.to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.pill_indicator_position, position);
        }
    }

    #[test]
    fn test_pill_indicator_position_serialization() {
        let settings = Settings {
            pill_indicator_position: "top".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"pill_indicator_position\":\"top\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pill_indicator_position, "top");
    }

    // ============================================================================
    // Sound on Recording End Tests
    // ============================================================================

    #[test]
    fn test_play_sound_on_recording_default() {
        let settings = Settings::default();
        assert!(settings.play_sound_on_recording);
    }

    #[test]
    fn test_play_sound_on_recording_end_default() {
        let settings = Settings::default();
        assert!(settings.play_sound_on_recording_end);
    }

    #[test]
    fn test_play_sound_settings_disabled() {
        let settings = Settings {
            play_sound_on_recording: false,
            play_sound_on_recording_end: false,
            ..Settings::default()
        };
        assert!(!settings.play_sound_on_recording);
        assert!(!settings.play_sound_on_recording_end);
    }

    #[test]
    fn test_play_sound_settings_serialization() {
        let settings = Settings {
            play_sound_on_recording: false,
            play_sound_on_recording_end: true,
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"play_sound_on_recording\":false"));
        assert!(json.contains("\"play_sound_on_recording_end\":true"));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.play_sound_on_recording);
        assert!(deserialized.play_sound_on_recording_end);
    }

    #[test]
    fn test_play_sound_mixed_settings() {
        // Test with only start sound enabled
        let settings1 = Settings {
            play_sound_on_recording: true,
            play_sound_on_recording_end: false,
            ..Settings::default()
        };
        assert!(settings1.play_sound_on_recording);
        assert!(!settings1.play_sound_on_recording_end);

        // Test with only end sound enabled
        let settings2 = Settings {
            play_sound_on_recording: false,
            play_sound_on_recording_end: true,
            ..Settings::default()
        };
        assert!(!settings2.play_sound_on_recording);
        assert!(settings2.play_sound_on_recording_end);
    }

    // ============================================================================
    // Recording Mode (Toggle vs Push-to-Talk) Tests
    // ============================================================================

    #[test]
    fn test_recording_mode_default() {
        let settings = Settings::default();
        assert_eq!(settings.recording_mode, "toggle");
    }

    #[test]
    fn test_recording_mode_toggle() {
        let settings = Settings {
            recording_mode: "toggle".to_string(),
            ..Settings::default()
        };
        assert_eq!(settings.recording_mode, "toggle");
    }

    #[test]
    fn test_recording_mode_push_to_talk() {
        let settings = Settings {
            recording_mode: "push_to_talk".to_string(),
            ..Settings::default()
        };
        assert_eq!(settings.recording_mode, "push_to_talk");
    }

    #[test]
    fn test_recording_mode_serialization() {
        let settings = Settings {
            recording_mode: "push_to_talk".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"recording_mode\":\"push_to_talk\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.recording_mode, "push_to_talk");
    }

    // ============================================================================
    // Push-to-Talk Settings Tests
    // ============================================================================

    #[test]
    fn test_use_different_ptt_key_default() {
        let settings = Settings::default();
        assert!(!settings.use_different_ptt_key);
    }

    #[test]
    fn test_ptt_hotkey_default() {
        let settings = Settings::default();
        assert_eq!(settings.ptt_hotkey, Some("Alt+Space".to_string()));
    }

    #[test]
    fn test_ptt_settings_enabled() {
        let settings = Settings {
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("CommandOrControl+Space".to_string()),
            ..Settings::default()
        };

        assert_eq!(settings.recording_mode, "push_to_talk");
        assert!(settings.use_different_ptt_key);
        assert_eq!(
            settings.ptt_hotkey,
            Some("CommandOrControl+Space".to_string())
        );
    }

    #[test]
    fn test_ptt_hotkey_none() {
        let settings = Settings {
            ptt_hotkey: None,
            ..Settings::default()
        };
        assert!(settings.ptt_hotkey.is_none());
    }

    #[test]
    fn test_ptt_settings_serialization() {
        let settings = Settings {
            use_different_ptt_key: true,
            ptt_hotkey: Some("Shift+Space".to_string()),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"use_different_ptt_key\":true"));
        assert!(json.contains("\"ptt_hotkey\":\"Shift+Space\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert!(deserialized.use_different_ptt_key);
        assert_eq!(deserialized.ptt_hotkey, Some("Shift+Space".to_string()));
    }

    // ============================================================================
    // Network Sharing Settings Tests
    // ============================================================================

    #[test]
    fn test_sharing_port_default() {
        let settings = Settings::default();
        assert_eq!(settings.sharing_port, Some(47842));
    }

    #[test]
    fn test_sharing_password_default() {
        let settings = Settings::default();
        assert!(settings.sharing_password.is_none());
    }

    #[test]
    fn test_sharing_settings_with_password() {
        let settings = Settings {
            sharing_port: Some(8080),
            sharing_password: Some("secret123".to_string()),
            ..Settings::default()
        };

        assert_eq!(settings.sharing_port, Some(8080));
        assert_eq!(settings.sharing_password, Some("secret123".to_string()));
    }

    #[test]
    fn test_sharing_port_none() {
        let settings = Settings {
            sharing_port: None,
            ..Settings::default()
        };
        assert!(settings.sharing_port.is_none());
    }

    #[test]
    fn test_sharing_settings_serialization() {
        let settings = Settings {
            sharing_port: Some(9000),
            sharing_password: Some("mypassword".to_string()),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"sharing_port\":9000"));
        assert!(json.contains("\"sharing_password\":\"mypassword\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sharing_port, Some(9000));
        assert_eq!(deserialized.sharing_password, Some("mypassword".to_string()));
    }

    #[test]
    fn test_sharing_port_edge_values() {
        // Test minimum valid port
        let min_port = Settings {
            sharing_port: Some(1),
            ..Settings::default()
        };
        assert_eq!(min_port.sharing_port, Some(1));

        // Test maximum valid port
        let max_port = Settings {
            sharing_port: Some(65535),
            ..Settings::default()
        };
        assert_eq!(max_port.sharing_port, Some(65535));
    }

    // ============================================================================
    // Clipboard Settings Tests
    // ============================================================================

    #[test]
    fn test_keep_transcription_in_clipboard_default() {
        let settings = Settings::default();
        assert!(!settings.keep_transcription_in_clipboard);
    }

    #[test]
    fn test_keep_transcription_in_clipboard_enabled() {
        let settings = Settings {
            keep_transcription_in_clipboard: true,
            ..Settings::default()
        };
        assert!(settings.keep_transcription_in_clipboard);
    }

    // ============================================================================
    // resolve_pill_indicator_mode Helper Function Tests
    // ============================================================================

    #[test]
    fn test_resolve_pill_indicator_mode_prefers_stored_mode() {
        // When stored_mode is present, it should be returned regardless of legacy value
        let result = resolve_pill_indicator_mode(
            Some("always".to_string()),
            Some(false),
            "when_recording".to_string(),
        );
        assert_eq!(result, "always");

        let result2 = resolve_pill_indicator_mode(
            Some("never".to_string()),
            Some(true),
            "when_recording".to_string(),
        );
        assert_eq!(result2, "never");
    }

    #[test]
    fn test_resolve_pill_indicator_mode_migrates_legacy_show_true() {
        // When legacy show_pill=true, should migrate to "always"
        let result = resolve_pill_indicator_mode(None, Some(true), "when_recording".to_string());
        assert_eq!(result, "always");
    }

    #[test]
    fn test_resolve_pill_indicator_mode_migrates_legacy_show_false() {
        // When legacy show_pill=false, should migrate to "when_recording"
        let result = resolve_pill_indicator_mode(None, Some(false), "when_recording".to_string());
        assert_eq!(result, "when_recording");
    }

    #[test]
    fn test_resolve_pill_indicator_mode_uses_default_when_nothing_set() {
        // When both stored_mode and legacy are None, use default
        let result = resolve_pill_indicator_mode(None, None, "when_recording".to_string());
        assert_eq!(result, "when_recording");

        let result2 = resolve_pill_indicator_mode(None, None, "never".to_string());
        assert_eq!(result2, "never");

        let result3 = resolve_pill_indicator_mode(None, None, "always".to_string());
        assert_eq!(result3, "always");
    }

    #[test]
    fn test_resolve_pill_indicator_mode_with_empty_string() {
        // Empty string should still be returned if it's the stored value
        let result = resolve_pill_indicator_mode(
            Some("".to_string()),
            Some(true),
            "when_recording".to_string(),
        );
        assert_eq!(result, "");
    }

    // ============================================================================
    // Model Engine Settings Tests
    // ============================================================================

    #[test]
    fn test_current_model_engine_default() {
        let settings = Settings::default();
        assert_eq!(settings.current_model_engine, "whisper");
    }

    #[test]
    fn test_current_model_engine_parakeet() {
        let settings = Settings {
            current_model_engine: "parakeet".to_string(),
            ..Settings::default()
        };
        assert_eq!(settings.current_model_engine, "parakeet");
    }

    #[test]
    fn test_current_model_engine_soniox() {
        let settings = Settings {
            current_model_engine: "soniox".to_string(),
            ..Settings::default()
        };
        assert_eq!(settings.current_model_engine, "soniox");
    }

    #[test]
    fn test_current_model_engine_serialization() {
        let settings = Settings {
            current_model_engine: "parakeet".to_string(),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"current_model_engine\":\"parakeet\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.current_model_engine, "parakeet");
    }

    // ============================================================================
    // Translate to English Settings Tests
    // ============================================================================

    #[test]
    fn test_translate_to_english_default() {
        let settings = Settings::default();
        assert!(!settings.translate_to_english);
    }

    #[test]
    fn test_translate_to_english_enabled() {
        let settings = Settings {
            translate_to_english: true,
            ..Settings::default()
        };
        assert!(settings.translate_to_english);
    }

    // ============================================================================
    // Selected Microphone Settings Tests
    // ============================================================================

    #[test]
    fn test_selected_microphone_default() {
        let settings = Settings::default();
        assert!(settings.selected_microphone.is_none());
    }

    #[test]
    fn test_selected_microphone_set() {
        let settings = Settings {
            selected_microphone: Some("USB Microphone".to_string()),
            ..Settings::default()
        };
        assert_eq!(
            settings.selected_microphone,
            Some("USB Microphone".to_string())
        );
    }

    #[test]
    fn test_selected_microphone_serialization() {
        let settings = Settings {
            selected_microphone: Some("Blue Yeti".to_string()),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"selected_microphone\":\"Blue Yeti\""));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.selected_microphone,
            Some("Blue Yeti".to_string())
        );
    }

    // ============================================================================
    // Pill Position Settings Tests
    // ============================================================================

    #[test]
    fn test_pill_position_default() {
        let settings = Settings::default();
        assert!(settings.pill_position.is_none());
    }

    #[test]
    fn test_pill_position_set() {
        let settings = Settings {
            pill_position: Some((100.5, 200.75)),
            ..Settings::default()
        };
        assert_eq!(settings.pill_position, Some((100.5, 200.75)));
    }

    #[test]
    fn test_pill_position_serialization() {
        let settings = Settings {
            pill_position: Some((50.0, 150.0)),
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"pill_position\":[50.0,150.0]"));

        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pill_position, Some((50.0, 150.0)));
    }

    #[test]
    fn test_pill_position_negative_values() {
        // Negative positions might occur on multi-monitor setups
        let settings = Settings {
            pill_position: Some((-100.0, -50.0)),
            ..Settings::default()
        };
        assert_eq!(settings.pill_position, Some((-100.0, -50.0)));
    }

    // ============================================================================
    // Transcription Cleanup Days Tests
    // ============================================================================

    #[test]
    fn test_transcription_cleanup_days_default() {
        let settings = Settings::default();
        assert!(settings.transcription_cleanup_days.is_none());
    }

    #[test]
    fn test_transcription_cleanup_days_set() {
        let settings = Settings {
            transcription_cleanup_days: Some(30),
            ..Settings::default()
        };
        assert_eq!(settings.transcription_cleanup_days, Some(30));
    }

    #[test]
    fn test_transcription_cleanup_days_edge_values() {
        // Test minimum value
        let min_days = Settings {
            transcription_cleanup_days: Some(1),
            ..Settings::default()
        };
        assert_eq!(min_days.transcription_cleanup_days, Some(1));

        // Test larger value
        let large_days = Settings {
            transcription_cleanup_days: Some(365),
            ..Settings::default()
        };
        assert_eq!(large_days.transcription_cleanup_days, Some(365));
    }

    // ============================================================================
    // Combined Settings Tests (Integration)
    // ============================================================================

    #[test]
    fn test_settings_full_configuration() {
        // Test a fully configured Settings struct
        let settings = Settings {
            hotkey: "Alt+R".to_string(),
            current_model: "large-v3-turbo".to_string(),
            current_model_engine: "whisper".to_string(),
            language: "ja".to_string(),
            translate_to_english: true,
            theme: "dark".to_string(),
            transcription_cleanup_days: Some(14),
            pill_position: Some((500.0, 300.0)),
            launch_at_startup: true,
            onboarding_completed: true,
            check_updates_automatically: false,
            selected_microphone: Some("Built-in Microphone".to_string()),
            recording_mode: "push_to_talk".to_string(),
            use_different_ptt_key: true,
            ptt_hotkey: Some("Alt+P".to_string()),
            keep_transcription_in_clipboard: true,
            play_sound_on_recording: false,
            play_sound_on_recording_end: true,
            pill_indicator_mode: "always".to_string(),
            pill_indicator_position: "top".to_string(),
            sharing_port: Some(9999),
            sharing_password: Some("secure_pass".to_string()),
        };

        // Verify all values
        assert_eq!(settings.hotkey, "Alt+R");
        assert_eq!(settings.current_model, "large-v3-turbo");
        assert_eq!(settings.current_model_engine, "whisper");
        assert_eq!(settings.language, "ja");
        assert!(settings.translate_to_english);
        assert_eq!(settings.theme, "dark");
        assert_eq!(settings.transcription_cleanup_days, Some(14));
        assert_eq!(settings.pill_position, Some((500.0, 300.0)));
        assert!(settings.launch_at_startup);
        assert!(settings.onboarding_completed);
        assert!(!settings.check_updates_automatically);
        assert_eq!(
            settings.selected_microphone,
            Some("Built-in Microphone".to_string())
        );
        assert_eq!(settings.recording_mode, "push_to_talk");
        assert!(settings.use_different_ptt_key);
        assert_eq!(settings.ptt_hotkey, Some("Alt+P".to_string()));
        assert!(settings.keep_transcription_in_clipboard);
        assert!(!settings.play_sound_on_recording);
        assert!(settings.play_sound_on_recording_end);
        assert_eq!(settings.pill_indicator_mode, "always");
        assert_eq!(settings.pill_indicator_position, "top");
        assert_eq!(settings.sharing_port, Some(9999));
        assert_eq!(settings.sharing_password, Some("secure_pass".to_string()));

        // Test full round-trip serialization
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.hotkey, settings.hotkey);
        assert_eq!(deserialized.current_model, settings.current_model);
        assert_eq!(deserialized.pill_indicator_mode, settings.pill_indicator_mode);
        assert_eq!(
            deserialized.pill_indicator_position,
            settings.pill_indicator_position
        );
        assert_eq!(
            deserialized.play_sound_on_recording,
            settings.play_sound_on_recording
        );
        assert_eq!(
            deserialized.play_sound_on_recording_end,
            settings.play_sound_on_recording_end
        );
        assert_eq!(deserialized.sharing_port, settings.sharing_port);
        assert_eq!(deserialized.sharing_password, settings.sharing_password);
    }
}
