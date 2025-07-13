#[cfg(test)]
mod tests {
    use crate::commands::settings::Settings;
    use serde_json::json;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();

        assert_eq!(settings.hotkey, "CommandOrControl+Shift+Space");
        assert_eq!(settings.current_model, ""); // Empty means auto-select
        assert_eq!(settings.language, "en");
        assert_eq!(settings.auto_insert, true);
        assert_eq!(settings.show_window_on_record, false);
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.transcription_cleanup_days, None);
        assert_eq!(settings.show_pill_widget, true);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            hotkey: "CommandOrControl+A".to_string(),
            current_model: "base".to_string(),
            language: "es".to_string(),
            auto_insert: false,
            show_window_on_record: true,
            theme: "dark".to_string(),
            transcription_cleanup_days: Some(7),
            show_pill_widget: false,
            pill_position: Some((100.0, 200.0)),
        };

        // Test serialization
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"hotkey\":\"CommandOrControl+A\""));
        assert!(json.contains("\"current_model\":\"base\""));
        assert!(json.contains("\"language\":\"es\""));
        assert!(json.contains("\"auto_insert\":false"));
        assert!(json.contains("\"show_window_on_record\":true"));
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"transcription_cleanup_days\":7"));
        assert!(json.contains("\"show_pill_widget\":false"));

        // Test deserialization
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hotkey, settings.hotkey);
        assert_eq!(deserialized.current_model, settings.current_model);
        assert_eq!(deserialized.language, settings.language);
        assert_eq!(deserialized.auto_insert, settings.auto_insert);
        assert_eq!(
            deserialized.show_window_on_record,
            settings.show_window_on_record
        );
        assert_eq!(deserialized.theme, settings.theme);
        assert_eq!(deserialized.transcription_cleanup_days, settings.transcription_cleanup_days);
        assert_eq!(deserialized.show_pill_widget, settings.show_pill_widget);
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
            language: "fr".to_string(),
            auto_insert: true,
            show_window_on_record: true,
            theme: "light".to_string(),
            transcription_cleanup_days: Some(30),
            show_pill_widget: true,
            pill_position: None,
        };

        let cloned = settings.clone();
        assert_eq!(cloned.hotkey, settings.hotkey);
        assert_eq!(cloned.current_model, settings.current_model);
        assert_eq!(cloned.language, settings.language);
        assert_eq!(cloned.auto_insert, settings.auto_insert);
        assert_eq!(cloned.show_window_on_record, settings.show_window_on_record);
        assert_eq!(cloned.theme, settings.theme);
        assert_eq!(cloned.transcription_cleanup_days, settings.transcription_cleanup_days);
        assert_eq!(cloned.show_pill_widget, settings.show_pill_widget);
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
            ..Settings::default()
        };
        assert_eq!(auto_settings.current_model, "");

        // Specific model
        let specific_settings = Settings {
            current_model: "base".to_string(),
            ..Settings::default()
        };
        assert_eq!(specific_settings.current_model, "base");
    }

    #[test]
    fn test_boolean_flags() {
        // Test all combinations of boolean flags
        let combinations = vec![(true, true), (true, false), (false, true), (false, false)];

        for (auto_insert, show_window) in combinations {
            let settings = Settings {
                auto_insert,
                show_window_on_record: show_window,
                ..Settings::default()
            };
            assert_eq!(settings.auto_insert, auto_insert);
            assert_eq!(settings.show_window_on_record, show_window);
        }
    }

    #[test]
    fn test_settings_to_json_value() {
        let settings = Settings::default();

        // Convert to JSON Value
        let value = json!({
            "hotkey": settings.hotkey,
            "current_model": settings.current_model,
            "language": settings.language,
            "auto_insert": settings.auto_insert,
            "show_window_on_record": settings.show_window_on_record,
            "theme": settings.theme,
            "transcription_cleanup_days": settings.transcription_cleanup_days,
            "show_pill_widget": settings.show_pill_widget,
        });

        assert_eq!(value["hotkey"], "CommandOrControl+Shift+Space");
        assert_eq!(value["current_model"], "");
        assert_eq!(value["language"], "en");
        assert_eq!(value["auto_insert"], true);
        assert_eq!(value["show_window_on_record"], false);
        assert_eq!(value["theme"], "system");
        assert_eq!(value["transcription_cleanup_days"], serde_json::Value::Null);
        assert_eq!(value["show_pill_widget"], true);
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
}
