use serde::{Deserialize, Serialize};

// Base prompt template with {language} placeholder
const BASE_PROMPT_TEMPLATE: &str = r#"You are a post-processor for voice transcripts.

Resolve self-corrections and intent changes: delete the retracted part and keep only the final intended phrasing (last-intent wins).
Tie-breakers:
- Prefer the last explicit affirmative directive ("we will", "let's", "I'll").
- For conflicting recipients/places/dates/numbers, keep the last stated value.
- Remove "or/maybe" alternatives that precede a final choice.
- If still uncertain, output the safest minimal intent without adding details.

Rewrite into clear, natural written {language} while preserving meaning and tone.
Remove fillers/false starts; fix grammar, punctuation, capitalization, and spacing.
Normalize obvious names/brands/terms when unambiguous; if uncertain, don't guess—keep generic.
Format numbers/dates/times as spoken. Handle dictation commands only when explicitly said (e.g., "period", "new line").
Output only the polished text."#;

/// Convert ISO 639-1 language code to full language name
pub fn get_language_name(code: &str) -> &'static str {
    match code.to_lowercase().as_str() {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "pl" => "Polish",
        "ru" => "Russian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "tr" => "Turkish",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        "ms" => "Malay",
        "sv" => "Swedish",
        "da" => "Danish",
        "no" => "Norwegian",
        "fi" => "Finnish",
        "cs" => "Czech",
        "sk" => "Slovak",
        "uk" => "Ukrainian",
        "el" => "Greek",
        "he" => "Hebrew",
        "ro" => "Romanian",
        "hu" => "Hungarian",
        "bg" => "Bulgarian",
        "hr" => "Croatian",
        "sr" => "Serbian",
        "sl" => "Slovenian",
        "lt" => "Lithuanian",
        "lv" => "Latvian",
        "et" => "Estonian",
        "bn" => "Bengali",
        "ta" => "Tamil",
        "te" => "Telugu",
        "mr" => "Marathi",
        "gu" => "Gujarati",
        "kn" => "Kannada",
        "ml" => "Malayalam",
        "pa" => "Punjabi",
        "ur" => "Urdu",
        "fa" => "Persian",
        "sw" => "Swahili",
        "af" => "Afrikaans",
        "ca" => "Catalan",
        "eu" => "Basque",
        "gl" => "Galician",
        "cy" => "Welsh",
        "is" => "Icelandic",
        "mt" => "Maltese",
        "sq" => "Albanian",
        "mk" => "Macedonian",
        "be" => "Belarusian",
        "ka" => "Georgian",
        "hy" => "Armenian",
        "az" => "Azerbaijani",
        "kk" => "Kazakh",
        "uz" => "Uzbek",
        "tl" => "Tagalog",
        "ne" => "Nepali",
        "si" => "Sinhala",
        "km" => "Khmer",
        "lo" => "Lao",
        "my" => "Burmese",
        "mn" => "Mongolian",
        _ => "English", // Default fallback
    }
}

/// Build the base prompt with the specified language
fn build_base_prompt(language: Option<&str>) -> String {
    let lang_name = language.map(get_language_name).unwrap_or("English");
    BASE_PROMPT_TEMPLATE.replace("{language}", lang_name)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum EnhancementPreset {
    PersonalDictation,
    CleanDictation,
    Writing,
    Notes,
    Message,
    Code,
}

impl EnhancementPreset {
    pub fn requires_ai_formatting(self) -> bool {
        !matches!(self, Self::PersonalDictation)
    }
}

pub fn migrate_preset_str(raw: &str, ai_enabled: bool) -> EnhancementPreset {
    match raw {
        "PersonalDictation" => EnhancementPreset::PersonalDictation,
        "CleanDictation" => EnhancementPreset::CleanDictation,
        "Writing" => EnhancementPreset::Writing,
        "Notes" => EnhancementPreset::Notes,
        "Message" => EnhancementPreset::Message,
        "Code" => EnhancementPreset::Code,
        "Coding" | "Prompts" | "Commit" => EnhancementPreset::Code,
        "Email" => EnhancementPreset::Writing,
        "Default" => {
            if ai_enabled {
                EnhancementPreset::CleanDictation
            } else {
                EnhancementPreset::PersonalDictation
            }
        }
        _ => EnhancementPreset::PersonalDictation,
    }
}

impl<'de> Deserialize<'de> for EnhancementPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(migrate_preset_str(&s, false))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancementOptions {
    pub preset: EnhancementPreset,
}

impl EnhancementOptions {
    pub fn default_for_ai_enabled(ai_enabled: bool) -> Self {
        Self {
            preset: if ai_enabled {
                EnhancementPreset::CleanDictation
            } else {
                EnhancementPreset::PersonalDictation
            },
        }
    }
}

impl Default for EnhancementOptions {
    fn default() -> Self {
        Self::default_for_ai_enabled(false)
    }
}

pub fn parse_enhancement_options_from_value(
    value: &serde_json::Value,
    ai_enabled: bool,
) -> Result<EnhancementOptions, String> {
    let preset_raw = value
        .get("preset")
        .and_then(|v| v.as_str())
        .unwrap_or("Default");

    Ok(EnhancementOptions {
        preset: migrate_preset_str(preset_raw, ai_enabled),
    })
}

pub fn build_enhancement_prompt(
    text: &str,
    context: Option<&str>,
    options: &EnhancementOptions,
    language: Option<&str>,
) -> String {
    let base_prompt = build_base_prompt(language);

    let mode_transform = match options.preset {
        EnhancementPreset::PersonalDictation | EnhancementPreset::CleanDictation => "",
        EnhancementPreset::Writing => WRITING_TRANSFORM,
        EnhancementPreset::Notes => NOTES_TRANSFORM,
        EnhancementPreset::Message => MESSAGE_TRANSFORM,
        EnhancementPreset::Code => CODE_TRANSFORM,
    };

    let mut prompt = if mode_transform.is_empty() {
        format!("{}\n\nTranscribed text:\n{}", base_prompt, text.trim())
    } else {
        format!(
            "{}\n\n{}\n\nTranscribed text:\n{}",
            base_prompt,
            mode_transform,
            text.trim()
        )
    };

    if let Some(ctx) = context {
        prompt.push_str(&format!("\n\nContext: {}", ctx));
    }

    prompt
}

const WRITING_TRANSFORM: &str = r#"Now refine the cleaned text for polished prose:
  - Improve flow and readability with smooth transitions.
  - Vary sentence structure; avoid repetition.
  - Strengthen word choice without changing meaning.
  - Maintain the speaker's original voice and intent.
  - Ensure consistent tense and point of view.
Return only the polished text."#;

const NOTES_TRANSFORM: &str = r#"Now organize the cleaned text into structured notes:
  - Extract key points as concise bullet items.
  - Group related ideas under clear headings.
  - Preserve all factual details, names, dates, and numbers.
  - Use hierarchical structure (topics → sub-points).
  - Include action items or decisions explicitly stated.
Return only the structured notes."#;

const MESSAGE_TRANSFORM: &str = r#"Now format the cleaned text as a concise message:
  - Lead with the key point or ask.
  - Keep it brief and scannable.
  - Match tone to intent (formal, casual, urgent).
  - Include greetings/closings only if the speaker provided them.
  - Preserve all names, links, and specifics verbatim.
Return only the formatted message."#;

const CODE_TRANSFORM: &str = r#"Now convert the cleaned text for a coding context:
  - For commit messages: use conventional format type(scope): description, present tense, no period, ≤72 chars.
  - For comments: be concise, use proper technical terminology.
  - For documentation: include purpose, parameters, and return values if applicable.
  - Preserve all code references, variable names, and technical terms verbatim.
Return only the formatted output."#;
