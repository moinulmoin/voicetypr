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

const EMAIL_PROMPT: &str = r#"Convert this into a professional email with proper structure. Add greeting, organize the content clearly, and include appropriate closing. Maintain professional tone.

Examples:
"need the report by friday please send it" →
"Subject: Report Request

Hi [Recipient],

Could you please send me the report by Friday?

Thank you,
[Sender]"

"meeting tomorrow 3pm about the project updates bring the slides" →
"Subject: Project Update Meeting - Tomorrow 3 PM

Hi team,

We have a project update meeting scheduled for tomorrow at 3 PM. Please bring your presentation slides.

See you there,
[Sender]"

Return ONLY the formatted email."#;

const COMMIT_PROMPT: &str = r#"Convert this into a conventional commit message. Use format: type(scope): description. Types: feat, fix, docs, style, refactor, test, chore.

Examples:
"fixed the login bug" → "fix(auth): resolve login authentication issue"
"added dark mode to settings" → "feat(ui): add dark mode toggle in settings"
"updated the readme" → "docs: update README with installation instructions"
"cleaned up old code" → "refactor: remove deprecated utility functions"

Return ONLY the commit message."#;

const NOTES_PROMPT: &str = r#"Format this as organized notes. Detect lists and create bullets or numbers. Identify sections and add headers. Keep it scannable.

Examples:
"groceries milk bread eggs cheese" →
"Groceries:
• Milk
• Bread
• Eggs
• Cheese"

"deploy process first build then test then push to prod" →
"Deploy Process:
1. Build
2. Test
3. Push to production"

"bug fixes login error when password empty profile page crash on ios pagination broken" →
"Bug Fixes:
• Login error when password empty
• Profile page crash on iOS
• Pagination broken"

Return ONLY the formatted notes."#;