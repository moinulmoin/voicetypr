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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum EnhancementPreset {
    Default,
    Writing,
    Notes,
    Message,
    Coding,
}

impl<'de> Deserialize<'de> for EnhancementPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Default" => Ok(EnhancementPreset::Default),
            "Writing" => Ok(EnhancementPreset::Writing),
            "Notes" => Ok(EnhancementPreset::Notes),
            "Message" => Ok(EnhancementPreset::Message),
            "Coding" => Ok(EnhancementPreset::Coding),
            // Legacy compatibility — old presets silently map to nearest new profile
            "Prompts" => Ok(EnhancementPreset::Coding),
            "Email" => Ok(EnhancementPreset::Writing),
            "Commit" => Ok(EnhancementPreset::Coding),
            _ => Ok(EnhancementPreset::Default),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancementOptions {
    pub preset: EnhancementPreset,
}

impl Default for EnhancementOptions {
    fn default() -> Self {
        Self {
            preset: EnhancementPreset::Default,
        }
    }
}

pub fn build_enhancement_prompt(
    text: &str,
    context: Option<&str>,
    options: &EnhancementOptions,
    language: Option<&str>,
) -> String {
    // Base processing applies to ALL presets, with language-aware output
    let base_prompt = build_base_prompt(language);

    let mode_transform = match options.preset {
        EnhancementPreset::Default => "",
        EnhancementPreset::Writing => WRITING_TRANSFORM,
        EnhancementPreset::Notes => NOTES_TRANSFORM,
        EnhancementPreset::Message => MESSAGE_TRANSFORM,
        EnhancementPreset::Coding => CODING_TRANSFORM,
    };

    // Build the complete prompt
    let mut prompt = if mode_transform.is_empty() {
        // Default preset: just base processing
        format!("{}\n\nTranscribed text:\n{}", base_prompt, text.trim())
    } else {
        // Other presets: base + transform
        format!(
            "{}\n\n{}\n\nTranscribed text:\n{}",
            base_prompt,
            mode_transform,
            text.trim()
        )
    };

    // Add context if provided
    if let Some(ctx) = context {
        prompt.push_str(&format!("\n\nContext: {}", ctx));
    }

    prompt
}

// Transformation layer for Writing profile
const WRITING_TRANSFORM: &str = r#"Now refine the cleaned text for polished prose:
  - Improve flow and readability with smooth transitions.
  - Vary sentence structure; avoid repetition.
  - Strengthen word choice without changing meaning.
  - Maintain the speaker's original voice and intent.
  - Ensure consistent tense and point of view.
Return only the polished text."#;

// Transformation layer for Notes profile
const NOTES_TRANSFORM: &str = r#"Now organize the cleaned text into structured notes:
  - Extract key points as concise bullet items.
  - Group related ideas under clear headings.
  - Preserve all factual details, names, dates, and numbers.
  - Use hierarchical structure (topics → sub-points).
  - Include action items or decisions explicitly stated.
Return only the structured notes."#;

// Transformation layer for Message profile
const MESSAGE_TRANSFORM: &str = r#"Now format the cleaned text as a concise message:
  - Lead with the key point or ask.
  - Keep it brief and scannable.
  - Match tone to intent (formal, casual, urgent).
  - Include greetings/closings only if the speaker provided them.
  - Preserve all names, links, and specifics verbatim.
Return only the formatted message."#;

// Transformation layer for Coding profile
const CODING_TRANSFORM: &str = r#"Now convert the cleaned text for a coding context:
  - For commit messages: use conventional format type(scope): description, present tense, no period, ≤72 chars.
  - For comments: be concise, use proper technical terminology.
  - For documentation: include purpose, parameters, and return values if applicable.
  - Preserve all code references, variable names, and technical terms verbatim.
Return only the formatted output."#;
