use serde::{Deserialize, Serialize};

// Self-correction rules that apply to ALL presets
const SELF_CORRECTION_RULES: &str = r#"FIRST, handle self-corrections:
When speakers correct themselves mid-sentence (e.g., "send it to John... to Mary"), keep only the final version ("send it to Mary")."#;

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
    pub custom_vocabulary: Vec<String>,
}

impl Default for EnhancementOptions {
    fn default() -> Self {
        Self {
            preset: EnhancementPreset::Default,
            custom_vocabulary: vec![],
        }
    }
}

pub fn build_enhancement_prompt(
    text: &str,
    context: Option<&str>,
    options: &EnhancementOptions,
) -> String {
    // Base processing applies to ALL presets
    let base_processing = format!("{}\n\n{}", SELF_CORRECTION_RULES, DEFAULT_PROMPT);
    
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
        format!("{}\n\nTranscribed text:\n{}", base_processing, text.trim())
    } else {
        // Other presets: base + transform
        format!(
            "{}\n\n{}\n\nTranscribed text:\n{}",
            base_processing,
            mode_transform,
            text.trim()
        )
    };

    // Add context if provided
    if let Some(ctx) = context {
        prompt.push_str(&format!("\n\nContext: {}", ctx));
    }

    // Add custom vocabulary
    if !options.custom_vocabulary.is_empty() {
        prompt.push_str(&format!(
            "\n\nRecognize these terms: {}",
            options.custom_vocabulary.join(", ")
        ));
    }

    prompt
}

const DEFAULT_PROMPT: &str = r#"THEN clean up this voice transcription:

- Remove filler words and stutters
- Fix all errors: grammar, spelling, punctuation, word choice
- Fix logical inconsistencies and nonsensical phrases
- Correct informal speech if inappropriate (gonna → going to)
- Correct technical terms and proper nouns
- Format numbers, dates, times naturally
- Handle dictation commands when explicitly stated
- Keep the original tone and flow - don't restructure or change style

Return ONLY the cleaned text as natural dictation output."#;

// Thin transformation layer for Prompts preset
const PROMPTS_TRANSFORM: &str = r#"FINALLY, transform the cleaned text into a well-structured AI prompt:

IDENTIFY the type:
- Request: Add deliverables and success criteria
- Question: Clarify scope and depth needed
- Task: Include constraints and requirements

ENHANCE by:
- Adding "what/how/why" if missing
- Specifying output format if relevant
- Preserving all technical details

Examples:
"fix the login bug" → "Fix the login bug. Explain what caused it and show the code changes needed."
"make a todo app" → "Create a todo app with add, edit, delete, and mark complete functionality. Include basic UI."
"what's this do" → "Explain what this code does, its purpose, and key implementation details."

Return ONLY the enhanced prompt."#;

// Thin transformation layer for Email preset
const EMAIL_TRANSFORM: &str = r#"FINALLY, format the cleaned text as an email:

DETECT intent and tone:
- Request: Include clear action items
- Update: Lead with key information  
- Question: Be specific about what you need
- Formal: Use professional language
- Casual: Keep friendly but clear

STRUCTURE:
- Subject: Specific and action-oriented
- Greeting: Match relationship (Hi/Dear/Hello)
- Body: Paragraph breaks for readability
- Closing: Appropriate sign-off
- Use [Name] for placeholders

Examples:
"need the Q3 report by Friday" →
"Subject: Q3 Report Needed by Friday

Hi [Name],

Could you please send me the Q3 report by Friday?

Thanks,
[Your name]"

"following up on our discussion about the new feature yesterday" →
"Subject: Follow-up: New Feature Discussion

Hi [Name],

Following up on our discussion about the new feature yesterday.

[Your name]"

Return ONLY the formatted email."#;

// Thin transformation layer for Commit preset
const COMMIT_TRANSFORM: &str = r#"FINALLY, convert to conventional commit format:

FORMAT: type(scope): description

TYPES: feat, fix, docs, style, refactor, perf, test, chore, build, ci

RULES:
- Lowercase verb, present tense
- No period at end
- Under 72 characters
- Add ! for breaking changes

Examples:
"fixed the login bug" → "fix(auth): resolve login authentication failure"
"added dark mode" → "feat(ui): add dark mode toggle"
"updated readme" → "docs: update installation instructions"

Return ONLY the commit message."#;

