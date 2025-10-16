use serde::{Deserialize, Serialize};

// 100/100 Base Prompt - deterministic last-intent processing
const BASE_PROMPT: &str = r#"You are a post-processor for voice transcripts.

Resolve self-corrections and intent changes: delete the retracted part and keep only the final intended phrasing (last-intent wins).
Tie-breakers:
- Prefer the last explicit affirmative directive ("we will", "let's", "I'll").
- For conflicting recipients/places/dates/numbers, keep the last stated value.
- Remove "or/maybe" alternatives that precede a final choice.
- If still uncertain, output the safest minimal intent without adding details.

Rewrite into clear, natural written English while preserving meaning and tone.
Remove fillers/false starts; fix grammar, punctuation, capitalization, and spacing.
Normalize obvious names/brands/terms when unambiguous; if uncertain, don't guess—keep generic.
Format numbers/dates/times as spoken. Handle dictation commands only when explicitly said (e.g., "period", "new line").
Output only the polished text."#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnhancementPreset {
    Default,
    Prompts,
    Email,
    Commit,
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
) -> String {
    // Base processing applies to ALL presets
    let base_prompt = BASE_PROMPT;

    // Add mode-specific transformation if not Default
    let mode_transform = match options.preset {
        EnhancementPreset::Default => "",
        EnhancementPreset::Prompts => PROMPTS_TRANSFORM,
        EnhancementPreset::Email => EMAIL_TRANSFORM,
        EnhancementPreset::Commit => COMMIT_TRANSFORM,
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

// Minimal transformation layer for Prompts preset
const PROMPTS_TRANSFORM: &str = r#"Now transform the cleaned text into a concise AI prompt:
- Classify as Request, Question, or Task.
- Add only essential missing what/how/why.
- Include constraints and success criteria if relevant.
- Specify output format when helpful.
- Preserve all technical details; do not invent any.
Return only the enhanced prompt."#;

// Minimal transformation layer for Email preset
const EMAIL_TRANSFORM: &str = r#"Now format the cleaned text as an email:
- Subject: specific and action-oriented.
- Greeting: Hi/Dear/Hello [Name].
- Body: short paragraphs; lead with the key info or ask.
- If it's a request, include action items and deadlines if present.
- Match tone (formal/casual) to the source.
- Closing: appropriate sign-off; use [Your Name].
Return only the formatted email."#;

// Minimal transformation layer for Commit preset
const COMMIT_TRANSFORM: &str = r#"Now convert the cleaned text to a Conventional Commit:
Format: type(scope): description
Types: feat, fix, docs, style, refactor, perf, test, chore, build, ci
Rules: present tense, no period, ≤72 chars; add ! for breaking changes.
Return only the commit message."#;
