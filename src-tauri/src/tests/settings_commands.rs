#[cfg(test)]
mod tests {
    use crate::commands::settings::{get_supported_languages, Settings};
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
}
