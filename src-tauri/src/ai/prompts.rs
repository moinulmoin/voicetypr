use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnhancementPreset {
    Default,
    Prompts,
    Email,
    Commit,
    Notes,
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
    let base_prompt = match options.preset {
        EnhancementPreset::Default => DEFAULT_PROMPT,
        EnhancementPreset::Prompts => PROMPTS_PROMPT,
        EnhancementPreset::Email => EMAIL_PROMPT,
        EnhancementPreset::Commit => COMMIT_PROMPT,
        EnhancementPreset::Notes => NOTES_PROMPT,
    };

    let mut prompt = format!("{}\n\nTranscribed text:\n{}", base_prompt, text.trim());

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

const DEFAULT_PROMPT: &str = r#"Fix ONLY spelling, grammar, and punctuation errors. Correct technical terms. Remove filler words like "um", "uh", "like".

IMPORTANT: Do NOT change the meaning, rephrase sentences, or interpret what the speaker meant. Keep the exact same words and structure, just fix errors.

Examples:
"um so i'm using java script and python" → "So I'm using JavaScript and Python."
"the a p i returns jason data" → "The API returns JSON data."
"how long are you doing man" → "How long are you doing, man?"
"is everything okay is everything good" → "Is everything okay? Is everything good?"

Return ONLY the corrected text without any interpretation or rephrasing."#;

const PROMPTS_PROMPT: &str = r#"Transform this into a clear, actionable prompt. Add minimal context to make it specific but keep it concise.

Examples:
"fix the bug" → "Fix the bug and explain what caused it."

"make a todo app" → "Create a todo app with add/edit/delete tasks and status filtering."

"explain this" → "Explain what this does and how it works."

"implement dark mode" → "Add dark mode toggle to the app with system preference detection."

Return ONLY the enhanced prompt."#;

const EMAIL_PROMPT: &str = r#"Convert to professional email format. Add subject, greeting, clear message, and closing.

Examples:
"need the report by friday" → "Subject: Report Request | Hi [Name], Could you please send me the report by Friday? Thanks, [Your name]"

"meeting tomorrow 3pm bring slides" → "Subject: Meeting Tomorrow 3PM | Hi team, Meeting tomorrow at 3 PM. Please bring your slides. See you there, [Your name]"

"sorry for the delay here's the file" → "Subject: File Attached | Hi [Name], Apologies for the delay. Please find the file attached. Best regards, [Your name]"

Return ONLY the formatted email."#;

const COMMIT_PROMPT: &str = r#"Convert this into a conventional commit message. Use format: type(scope): description. Types: feat, fix, docs, style, refactor, test, chore.

Examples:
"fixed the login bug" → "fix(auth): resolve login authentication issue"
"added dark mode to settings" → "feat(ui): add dark mode toggle in settings"
"updated the readme" → "docs: update README with installation instructions"
"cleaned up old code" → "refactor: remove deprecated utility functions"

Return ONLY the commit message."#;

const NOTES_PROMPT: &str = r#"Format as clean, organized notes. Auto-detect structure: lists, steps, ideas. Add minimal formatting.

Examples:
"groceries milk bread eggs" → "Groceries: • Milk • Bread • Eggs"

"first build then test then deploy" → "Steps: 1. Build 2. Test 3. Deploy"

"meeting notes discussed budget timeline next steps follow up with john" → "Meeting Notes: • Discussed budget & timeline • Next steps: Follow up with John"

"todo fix bug write tests update docs" → "TODO: • Fix bug • Write tests • Update docs"

Return ONLY the formatted notes."#;