#[cfg(test)]
mod behavior_tests {

    #[test]
    fn test_enhancement_prompt_generation() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions};

        // Test with default options (English)
        let options = EnhancementOptions::default();
        let prompt = build_enhancement_prompt("hello world", None, &options, None);

        assert!(prompt.contains("hello world"));
        assert!(prompt.contains("post-processor for voice transcripts"));
        assert!(prompt.contains("written English")); // Default language

        // Test with context
        let prompt_with_context =
            build_enhancement_prompt("hello world", Some("Casual conversation"), &options, None);

        assert!(prompt_with_context.contains("Context: Casual conversation"));

        // Test with Spanish language
        let prompt_spanish = build_enhancement_prompt("hola mundo", None, &options, Some("es"));
        assert!(prompt_spanish.contains("written Spanish"));

        // Test with French language
        let prompt_french = build_enhancement_prompt("bonjour monde", None, &options, Some("fr"));
        assert!(prompt_french.contains("written French"));
    }

    #[test]
    fn test_enhancement_presets() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let text = "um hello world";

        // Test Clean Dictation preset
        let clean_options = EnhancementOptions {
            preset: EnhancementPreset::CleanDictation,
        };
        let clean_prompt = build_enhancement_prompt(text, None, &clean_options, None);
        assert!(clean_prompt.contains("post-processor for voice transcripts"));
        assert!(!clean_prompt.contains("Now refine"));

        // Test Personal Dictation uses base cleanup prompt when invoked
        let personal_options = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let personal_prompt = build_enhancement_prompt(text, None, &personal_options, None);
        assert!(personal_prompt.contains("post-processor for voice transcripts"));
        assert!(!personal_prompt.contains("Now refine"));

        // Test Writing preset
        let writing_options = EnhancementOptions {
            preset: EnhancementPreset::Writing,
        };
        let writing_prompt = build_enhancement_prompt(text, None, &writing_options, None);
        assert!(writing_prompt.contains("refine the cleaned text for polished prose"));

        // Test Notes preset
        let notes_options = EnhancementOptions {
            preset: EnhancementPreset::Notes,
        };
        let notes_prompt = build_enhancement_prompt(text, None, &notes_options, None);
        assert!(notes_prompt.contains("organize the cleaned text into structured notes"));

        // Test Message preset
        let message_options = EnhancementOptions {
            preset: EnhancementPreset::Message,
        };
        let message_prompt = build_enhancement_prompt(text, None, &message_options, None);
        assert!(message_prompt.contains("format the cleaned text as a concise message"));

        // Test Code preset
        let code_options = EnhancementOptions {
            preset: EnhancementPreset::Code,
        };
        let code_prompt = build_enhancement_prompt(text, None, &code_options, None);
        assert!(code_prompt.contains("convert the cleaned text for a coding context"));
    }

    #[test]
    fn test_self_correction_rules_in_all_presets() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "send it to john... to mary";

        // Test that ALL presets include self-correction rules
        let presets = vec![
            EnhancementPreset::PersonalDictation,
            EnhancementPreset::CleanDictation,
            EnhancementPreset::Writing,
            EnhancementPreset::Notes,
            EnhancementPreset::Message,
            EnhancementPreset::Code,
        ];

        for preset in presets {
            let options = EnhancementOptions { preset };
            let prompt = build_enhancement_prompt(test_text, None, &options, None);

            // All prompts should include self-correction rules
            assert!(
                prompt.contains("self-corrections"),
                "Preset {:?} should include self-correction rules",
                preset
            );
        }
    }

    #[test]
    fn test_layered_architecture() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "test";

        // Test that all presets include base processing
        let presets = vec![
            EnhancementPreset::PersonalDictation,
            EnhancementPreset::CleanDictation,
            EnhancementPreset::Writing,
            EnhancementPreset::Notes,
            EnhancementPreset::Message,
            EnhancementPreset::Code,
        ];

        for preset in presets {
            let options = EnhancementOptions { preset };
            let prompt = build_enhancement_prompt(test_text, None, &options, None);

            // All should include self-correction rules
            assert!(
                prompt.contains("self-corrections"),
                "Preset {:?} should include self-correction rules",
                preset
            );

            // All should include base processing
            assert!(
                prompt.contains("post-processor for voice transcripts"),
                "Preset {:?} should include base processing",
                preset
            );

            // AI transform presets should have transformation instruction
            if matches!(
                preset,
                EnhancementPreset::Writing
                    | EnhancementPreset::Notes
                    | EnhancementPreset::Message
                    | EnhancementPreset::Code
            ) {
                assert!(
                    prompt.contains("Now"),
                    "Preset {:?} should have transformation",
                    preset
                );
            }
        }
    }

    #[test]
    fn test_default_prompt_comprehensive_features() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "test transcription";
        let options = EnhancementOptions {
            preset: EnhancementPreset::CleanDictation,
        };
        let prompt = build_enhancement_prompt(test_text, None, &options, None);

        // Test that Clean Dictation prompt includes all comprehensive features

        // 1. Self-correction handling
        assert!(
            prompt.contains("self-corrections"),
            "Should handle self-corrections"
        );
        assert!(
            prompt.contains("last-intent wins"),
            "Should use last-intent policy"
        );

        // 2. Error correction
        assert!(
            prompt.contains("grammar, punctuation, capitalization"),
            "Should handle grammar and spelling"
        );

        // 3. Number and time formatting
        assert!(
            prompt.contains("numbers/dates/times as spoken"),
            "Should format numbers and dates"
        );

        // 4. Technical terms
        assert!(
            prompt.contains("Normalize obvious names/brands/terms"),
            "Should normalize technical terms"
        );
    }

    #[test]
    fn test_language_name_mapping() {
        use crate::ai::prompts::get_language_name;

        // Test common languages
        assert_eq!(get_language_name("en"), "English");
        assert_eq!(get_language_name("es"), "Spanish");
        assert_eq!(get_language_name("fr"), "French");
        assert_eq!(get_language_name("de"), "German");
        assert_eq!(get_language_name("ja"), "Japanese");
        assert_eq!(get_language_name("zh"), "Chinese");
        assert_eq!(get_language_name("ar"), "Arabic");
        assert_eq!(get_language_name("hi"), "Hindi");
        assert_eq!(get_language_name("pt"), "Portuguese");
        assert_eq!(get_language_name("ru"), "Russian");

        // Test case insensitivity
        assert_eq!(get_language_name("EN"), "English");
        assert_eq!(get_language_name("Es"), "Spanish");

        // Test fallback for unknown language codes
        assert_eq!(get_language_name("xyz"), "English");
        assert_eq!(get_language_name(""), "English");
    }

    #[test]
    fn test_language_aware_prompts() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let options = EnhancementOptions {
            preset: EnhancementPreset::CleanDictation,
        };
        let text = "test text";

        // English (default)
        let prompt_en = build_enhancement_prompt(text, None, &options, Some("en"));
        assert!(prompt_en.contains("written English"));

        // Spanish
        let prompt_es = build_enhancement_prompt(text, None, &options, Some("es"));
        assert!(prompt_es.contains("written Spanish"));

        // Japanese
        let prompt_ja = build_enhancement_prompt(text, None, &options, Some("ja"));
        assert!(prompt_ja.contains("written Japanese"));

        // None defaults to English
        let prompt_none = build_enhancement_prompt(text, None, &options, None);
        assert!(prompt_none.contains("written English"));
    }

    #[test]
    fn test_preset_migration() {
        use crate::ai::prompts::{migrate_preset_str, EnhancementPreset};

        assert_eq!(
            migrate_preset_str("Default", false),
            EnhancementPreset::PersonalDictation
        );
        assert_eq!(
            migrate_preset_str("Default", true),
            EnhancementPreset::CleanDictation
        );
        assert_eq!(migrate_preset_str("Coding", false), EnhancementPreset::Code);
        assert_eq!(
            migrate_preset_str("Prompts", false),
            EnhancementPreset::Code
        );
        assert_eq!(migrate_preset_str("Commit", false), EnhancementPreset::Code);
        assert_eq!(
            migrate_preset_str("Email", false),
            EnhancementPreset::Writing
        );
        assert_eq!(
            migrate_preset_str("Unknown", false),
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_default_options_respect_ai_enabled() {
        use crate::ai::prompts::{EnhancementOptions, EnhancementPreset};

        assert_eq!(
            EnhancementOptions::default_for_ai_enabled(false).preset,
            EnhancementPreset::PersonalDictation
        );
        assert_eq!(
            EnhancementOptions::default_for_ai_enabled(true).preset,
            EnhancementPreset::CleanDictation
        );
    }

    #[test]
    fn test_legacy_preset_deserialization() {
        use crate::ai::prompts::EnhancementPreset;

        let preset: EnhancementPreset = serde_json::from_str("\"Prompts\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Code));

        let preset: EnhancementPreset = serde_json::from_str("\"Email\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Writing));

        let preset: EnhancementPreset = serde_json::from_str("\"Commit\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Code));

        let preset: EnhancementPreset = serde_json::from_str("\"Default\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::PersonalDictation));

        let preset: EnhancementPreset = serde_json::from_str("\"Unknown\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::PersonalDictation));

        for (json, expected) in [
            (
                "\"PersonalDictation\"",
                EnhancementPreset::PersonalDictation,
            ),
            ("\"CleanDictation\"", EnhancementPreset::CleanDictation),
            ("\"Writing\"", EnhancementPreset::Writing),
            ("\"Notes\"", EnhancementPreset::Notes),
            ("\"Message\"", EnhancementPreset::Message),
            ("\"Code\"", EnhancementPreset::Code),
        ] {
            let preset: EnhancementPreset = serde_json::from_str(json).unwrap();
            assert_eq!(preset, expected, "Failed for {}", json);
        }

        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Code).unwrap(),
            "\"Code\""
        );
        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Message).unwrap(),
            "\"Message\""
        );
    }

    #[test]
    fn test_parse_enhancement_options_from_value() {
        use crate::ai::prompts::{parse_enhancement_options_from_value, EnhancementPreset};

        let value = serde_json::json!({"preset": "Default"});
        let options = parse_enhancement_options_from_value(&value, true).unwrap();
        assert_eq!(options.preset, EnhancementPreset::CleanDictation);

        let options = parse_enhancement_options_from_value(&value, false).unwrap();
        assert_eq!(options.preset, EnhancementPreset::PersonalDictation);
    }

    #[test]
    fn test_legacy_options_deserialization() {
        use crate::ai::prompts::EnhancementOptions;

        let json = r#"{"preset":"Commit"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        let prompt =
            crate::ai::prompts::build_enhancement_prompt("fix login bug", None, &opts, None);
        assert!(prompt.contains("coding context"));

        let json = r#"{"preset":"Email"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        let prompt = crate::ai::prompts::build_enhancement_prompt("hello team", None, &opts, None);
        assert!(prompt.contains("polished prose"));
    }

    #[test]
    fn test_effective_enhancement_options_prefers_override() {
        use crate::ai::prompts::{
            effective_enhancement_options, EnhancementOptions, EnhancementPreset,
        };

        let stored = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let effective = effective_enhancement_options(&stored, Some(EnhancementPreset::Message));

        assert_eq!(effective.preset, EnhancementPreset::Message);
        assert!(effective.preset.requires_ai_formatting());
    }

    #[test]
    fn test_effective_enhancement_options_keeps_global_personal_without_override() {
        use crate::ai::prompts::{
            effective_enhancement_options, EnhancementOptions, EnhancementPreset,
        };

        let stored = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let effective = effective_enhancement_options(&stored, None);

        assert_eq!(effective.preset, EnhancementPreset::PersonalDictation);
        assert!(!effective.preset.requires_ai_formatting());
    }

    #[test]
    fn test_forced_message_preset_uses_message_prompt_with_global_personal() {
        use crate::ai::prompts::{
            build_enhancement_prompt, effective_enhancement_options, EnhancementOptions,
            EnhancementPreset,
        };

        let stored = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let effective = effective_enhancement_options(&stored, Some(EnhancementPreset::Message));
        let prompt = build_enhancement_prompt("hello world", None, &effective, None);

        assert!(prompt.contains("format the cleaned text as a concise message"));
    }

    #[test]
    fn test_manual_personal_preset_skips_ai_formatting_prompt_transform() {
        use crate::ai::prompts::{
            build_enhancement_prompt, effective_enhancement_options, EnhancementOptions,
            EnhancementPreset,
        };

        let stored = EnhancementOptions {
            preset: EnhancementPreset::PersonalDictation,
        };
        let effective = effective_enhancement_options(&stored, None);
        let prompt = build_enhancement_prompt("hello world", None, &effective, None);

        assert!(!prompt.contains("format the cleaned text as a concise message"));
        assert!(!effective.preset.requires_ai_formatting());
    }

    #[test]
    fn test_personal_dictation_does_not_require_ai() {
        use crate::ai::prompts::EnhancementPreset;

        assert!(!EnhancementPreset::PersonalDictation.requires_ai_formatting());
        assert!(EnhancementPreset::CleanDictation.requires_ai_formatting());
    }
}
