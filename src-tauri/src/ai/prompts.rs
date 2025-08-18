use serde::{Deserialize, Serialize};

// Self-correction rules that apply to ALL presets
const SELF_CORRECTION_RULES: &str = r#"FIRST, handle natural speech self-corrections:

SELF-CORRECTION PATTERNS (apply these before other processing):
1. Immediate replacement: "send it to John... to Mary" → "send it to Mary"
2. Trailing corrections: "meeting at 3pm... 4pm" → "meeting at 4pm"
3. Partial restarts: "the proj... the project deadline" → "the project deadline"
4. Negation patterns: "delete it... no rename it" → "rename it"
5. Or/actually patterns: "Tuesday... or actually Wednesday" → "Wednesday"
6. Wait/hold patterns: "send it now... wait tomorrow" → "send it tomorrow"
7. Cascading corrections: keep only the final version when multiple corrections occur

KEEP ONLY THE FINAL CORRECTION when speakers naturally correct themselves."#;

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

const DEFAULT_PROMPT: &str = r#"THEN clean up this voice transcription like a high-quality dictation tool would - fix errors while keeping the natural speech flow.

APPLY THESE CORRECTIONS:

1. REMOVE speech artifacts:
   - Filler words: um, uh, ah, er, hmm, like (when filler), you know (when filler)
   - False starts and incomplete thoughts
   - Unintentional word repetitions and stutters: "I I think", "that that's"
   - Verbal backspacing: "wait", "scratch that", "let me rephrase"

2. FIX common errors:
   - Homophones: there/their/they're, to/too/two, your/you're, its/it's, then/than
   - Grammar: subject-verb agreement, article usage (a/an/the)
   - Technical terms (fix misheard/misspelled tech words):
     * "java script" → "JavaScript", "type script" → "TypeScript", "react js" → "React.js"
     * "fortend/frontent/front and" → "frontend"
     * "backing/back and/beck end" → "backend"
     * "jason/jayson" → "JSON", "A P I" → "API"
     * Common programming terms that sound similar
   - Contractions: "dont" → "don't", "wont" → "won't", "cant" → "can't"
   - Word boundaries: "alot" → "a lot", "incase" → "in case"

3. ADD proper formatting:
   - Capitalize sentence beginnings and proper nouns (names, places, brands)
   - Add punctuation based on speech patterns and context
   - Numbers: "twenty twenty five" → "2025", "one hundred" → "100"
   - Times: "two thirty PM" → "2:30 PM"
   - Dates: "january first" → "January 1st"
   - Split run-on sentences at natural break points

4. HANDLE explicit dictation commands:
   - "period" or "full stop" → . (only when clearly dictating)
   - "comma" → , (only when clearly dictating)
   - "question mark" → ?
   - "new paragraph" → create paragraph break
   - Email addresses: "john at gmail dot com" → "john@gmail.com"

5. KEEP the natural flow:
   - Don't restructure into lists or bullet points
   - Keep the conversational sequence intact
   - Preserve the speaker's tone and style
   - Don't make casual speech overly formal
   - Maintain original sentence connections

EXAMPLES:
"um their going too the store at two thirty PM and there buying to apples" 
→ "They're going to the store at 2:30 PM and they're buying two apples."

"the java script function returns uh jason data with a two hundred status"
→ "The JavaScript function returns JSON data with a 200 status."

"first we need milk second bread third eggs"
→ "First, we need milk, second bread, third eggs."

"can you send it to john at gmail dot com"
→ "Can you send it to john@gmail.com?"

"I I think that that's the last no wait the least important one"
→ "I think that's the least important one."

Return ONLY the cleaned text as natural dictation output."#;

// Thin transformation layer for Prompts preset
const PROMPTS_TRANSFORM: &str = r#"FINALLY, transform the cleaned text into a well-structured AI prompt:

RULES:
- Keep the core request intact
- Make implicit requirements explicit
- Add output format if mentioned
- Don't over-elaborate if already clear
- Preserve technical level

Examples:
"fix the login bug" → "Fix the login bug. Include what caused it and what you changed."
"make a todo app" → "Create a todo app with basic features: add, edit, delete tasks, and mark as complete."
"explain this code" → "Explain what this code does, its purpose, and how it works."

Return ONLY the enhanced prompt."#;

// Thin transformation layer for Email preset
const EMAIL_TRANSFORM: &str = r#"FINALLY, format the cleaned text as an email:

ADD ONLY:
- Subject line (concise, specific)
- Greeting (match formality to content)
- Paragraph breaks for clarity
- Closing signature
- Use [Name] placeholders where needed

Example:
"Need the report by Friday" →
"Subject: Report Due Friday

Hi [Name],

Could you please send me the report by Friday?

Thanks,
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

