#[cfg(test)]
mod tests {

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

        // Test Default preset
        let default_options = EnhancementOptions::default();
        let default_prompt = build_enhancement_prompt(text, None, &default_options, None);
        assert!(default_prompt.contains("post-processor for voice transcripts"));

        // Test Writing preset
        let mut writing_options = EnhancementOptions::default();
        writing_options.preset = EnhancementPreset::Writing;
        let writing_prompt = build_enhancement_prompt(text, None, &writing_options, None);
        assert!(writing_prompt.contains("refine the cleaned text for polished prose"));

        // Test Notes preset
        let mut notes_options = EnhancementOptions::default();
        notes_options.preset = EnhancementPreset::Notes;
        let notes_prompt = build_enhancement_prompt(text, None, &notes_options, None);
        assert!(notes_prompt.contains("organize the cleaned text into structured notes"));

        // Test Message preset
        let mut message_options = EnhancementOptions::default();
        message_options.preset = EnhancementPreset::Message;
        let message_prompt = build_enhancement_prompt(text, None, &message_options, None);
        assert!(message_prompt.contains("format the cleaned text as a concise message"));

        // Test Coding preset
        let mut coding_options = EnhancementOptions::default();
        coding_options.preset = EnhancementPreset::Coding;
        let coding_prompt = build_enhancement_prompt(text, None, &coding_options, None);
        assert!(coding_prompt.contains("convert the cleaned text for a coding context"));
    }

    #[test]
    fn test_self_correction_rules_in_all_presets() {
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions, EnhancementPreset};

        let test_text = "send it to john... to mary";

        // Test that ALL presets include self-correction rules
        let presets = vec![
            EnhancementPreset::Default,
            EnhancementPreset::Writing,
            EnhancementPreset::Notes,
            EnhancementPreset::Message,
            EnhancementPreset::Coding,
        ];

        for preset in presets {
            let mut options = EnhancementOptions::default();
            options.preset = preset.clone();
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
            EnhancementPreset::Default,
            EnhancementPreset::Writing,
            EnhancementPreset::Notes,
            EnhancementPreset::Message,
            EnhancementPreset::Coding,
        ];

        for preset in presets {
            let mut options = EnhancementOptions::default();
            options.preset = preset.clone();
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

            // Non-default presets should have transformation instruction
            if !matches!(preset, EnhancementPreset::Default) {
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
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions};

        let test_text = "test transcription";
        let options = EnhancementOptions::default();
        let prompt = build_enhancement_prompt(test_text, None, &options, None);

        // Test that Default prompt includes all comprehensive features

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
        use crate::ai::prompts::{build_enhancement_prompt, EnhancementOptions};

        let options = EnhancementOptions::default();
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
    fn test_legacy_preset_deserialization() {
        use crate::ai::prompts::EnhancementPreset;

        // Legacy "Prompts" deserializes to Coding
        let preset: EnhancementPreset = serde_json::from_str("\"Prompts\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Coding));

        // Legacy "Email" deserializes to Writing
        let preset: EnhancementPreset = serde_json::from_str("\"Email\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Writing));

        // Legacy "Commit" deserializes to Coding
        let preset: EnhancementPreset = serde_json::from_str("\"Commit\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Coding));

        // Unknown values fall back to Default
        let preset: EnhancementPreset = serde_json::from_str("\"Unknown\"").unwrap();
        assert!(matches!(preset, EnhancementPreset::Default));

        // New presets deserialize correctly
        for (json, expected) in [
            ("\"Default\"", EnhancementPreset::Default),
            ("\"Writing\"", EnhancementPreset::Writing),
            ("\"Notes\"", EnhancementPreset::Notes),
            ("\"Message\"", EnhancementPreset::Message),
            ("\"Coding\"", EnhancementPreset::Coding),
        ] {
            let preset: EnhancementPreset = serde_json::from_str(json).unwrap();
            assert_eq!(preset, expected, "Failed for {}", json);
        }

        // Serialization always outputs new names (never legacy)
        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Coding).unwrap(),
            "\"Coding\""
        );
        assert_eq!(
            serde_json::to_string(&EnhancementPreset::Message).unwrap(),
            "\"Message\""
        );
    }

    #[test]
    fn test_legacy_options_deserialization() {
        use crate::ai::prompts::EnhancementOptions;

        // Legacy JSON with "Commit" preset stored in settings should load without error
        let json = r#"{"preset":"Commit"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        // Commit maps to Coding, so the prompt builder should work
        let prompt =
            crate::ai::prompts::build_enhancement_prompt("fix login bug", None, &opts, None);
        assert!(prompt.contains("coding context"));

        // Legacy "Email" options
        let json = r#"{"preset":"Email"}"#;
        let opts: EnhancementOptions = serde_json::from_str(json).unwrap();
        let prompt = crate::ai::prompts::build_enhancement_prompt("hello team", None, &opts, None);
        assert!(prompt.contains("polished prose"));
    }
}
