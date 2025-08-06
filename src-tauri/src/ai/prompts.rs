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

const DEFAULT_PROMPT: &str = r#"Clean up this voice transcription while preserving the speaker's exact meaning and intent.

CRITICAL RULES:
1. Fix grammar, spelling, and add punctuation (. , ? ! : ; -)
2. Remove ONLY filler words: um, uh, ah, er, like (when used as filler), you know, I mean, basically, actually (when redundant)
3. Correct technical terms and proper nouns (e.g., "java script" → "JavaScript", "react js" → "React.js")
4. Fix contractions and word boundaries (e.g., "dont" → "don't", "alot" → "a lot")
5. NEVER change meaning, add content, or remove important words
6. Preserve emphasis and tone markers
7. Keep numbers as spoken unless obviously wrong (e.g., "two thousand twenty five" → "2025")

PRESERVE:
- Exact phrasing and word choice (except fillers)
- Questions should remain questions
- Statements should remain statements
- Technical accuracy over formal grammar when they conflict

Examples:
"um so i'm using java script react and python for the a p i" → "So I'm using JavaScript, React, and Python for the API."
"the function returns uh jason data with a two hundred status" → "The function returns JSON data with a 200 status."
"like how are you doing today john" → "How are you doing today, John?"
"we need to refactor the getuser i d function asap" → "We need to refactor the getUserID function ASAP."
"can you check if the p r passed c i" → "Can you check if the PR passed CI?"

Return ONLY the cleaned text."#;

const PROMPTS_PROMPT: &str = r#"Transform this spoken request into a well-structured prompt that preserves the user's intent while adding helpful clarity.

APPROACH:
1. Keep the core request intact - don't change what they're asking for
2. Add only essential context to prevent ambiguity
3. Make implicit requirements explicit
4. Preserve the user's technical level and terminology
5. Keep it concise - don't over-elaborate
6. If the request is already clear, minimal changes are fine

STRUCTURE:
- Start with the main action/request
- Add key specifications if needed
- Include output format if relevant
- Preserve any constraints mentioned

Examples:
"fix the login bug" → "Fix the login bug. Include what caused it and what you changed."

"make a todo app" → "Create a todo app with basic features: add, edit, delete tasks, and mark as complete."

"explain this code" → "Explain what this code does, its purpose, and how it works."

"implement dark mode" → "Implement dark mode with a toggle switch and system preference detection."

"write tests for the user service" → "Write unit tests for the user service covering main functionality and edge cases."

"help me debug this" → "Help debug this issue. Show me how to identify the problem and fix it."

"refactor this function" → "Refactor this function to improve readability and performance. Explain the changes."

Return ONLY the enhanced prompt."#;

const EMAIL_PROMPT: &str = r#"Convert this spoken message into a properly formatted email. Adapt the tone based on context.

FORMAT RULES:
1. Include a clear, specific subject line
2. Add appropriate greeting (formal or casual based on content)
3. Structure the message clearly with proper paragraphs
4. Include a suitable closing
5. Use [Name] only where a specific name is needed
6. Match formality to the context (business = formal, team = casual)

TONE GUIDELINES:
- Urgent matters: Direct and clear
- Apologies: Sincere and professional  
- Requests: Polite and specific
- Updates: Informative and organized
- Follow-ups: Friendly but purposeful

Examples:
"need the quarterly report by end of day friday" → 
"Subject: Quarterly Report - Due Friday EOD

Hi [Name],

Could you please send me the quarterly report by end of day Friday?

Thanks,
[Your name]"

"team meeting tomorrow 3pm conference room b bring your updates" → 
"Subject: Team Meeting - Tomorrow 3 PM, Conference Room B

Hi team,

Quick reminder about our meeting tomorrow at 3 PM in Conference Room B. Please bring your project updates.

See you there!
[Your name]"

"following up on my previous email about the contract did you have a chance to review it" →
"Subject: Follow-up: Contract Review

Hi [Name],

I wanted to follow up on my previous email regarding the contract. Have you had a chance to review it?

Please let me know if you have any questions or need additional information.

Best regards,
[Your name]"

Return ONLY the formatted email."#;

const COMMIT_PROMPT: &str = r#"Convert to a conventional commit message following the standard format.

FORMAT: type(scope): description

TYPES:
- feat: New feature or functionality
- fix: Bug fix or issue resolution
- docs: Documentation changes only
- style: Code style, formatting (no logic change)
- refactor: Code restructuring (no behavior change)
- perf: Performance improvements
- test: Test additions or corrections
- chore: Build, config, dependencies, etc.
- revert: Reverting previous commit
- build: Build system or dependencies
- ci: CI/CD configuration changes

RULES:
1. Description starts with lowercase verb (present tense)
2. No period at the end
3. Keep under 72 characters
4. Scope is optional but recommended (detect from context)
5. Add ! before : for breaking changes
6. Be specific but concise

Examples:
"fixed the login bug where users couldn't sign in" → "fix(auth): resolve login authentication failure"

"added dark mode to the settings page" → "feat(settings): add dark mode toggle"

"updated the readme with new installation steps" → "docs: update installation instructions in README"

"made the api calls faster by adding caching" → "perf(api): implement response caching for faster requests"

"removed the old user service code we don't use anymore" → "refactor: remove deprecated user service implementation"

"fixed typos in error messages" → "fix: correct typos in error messages"

"upgraded react to version 18" → "chore(deps): upgrade React to v18"

"added unit tests for the payment module" → "test(payment): add unit tests for payment processing"

"this breaks the api response format" → "feat(api)!: change response format for better consistency"

Return ONLY the commit message."#;

const NOTES_PROMPT: &str = r#"Convert spoken thoughts into well-structured notes. Intelligently detect the type and organize accordingly.

DETECTION RULES:
1. Lists: Items mentioned together (shopping, tasks, ideas)
2. Steps/Process: Sequential actions with order indicators
3. Meeting/Discussion: Mixed topics, decisions, action items
4. Ideas/Brainstorm: Concepts, possibilities, considerations
5. Schedule/Timeline: Time-based items, deadlines
6. Goals/Objectives: Targets, outcomes, milestones

FORMATTING:
- Use bullet points (•) for unordered lists
- Use numbers for sequential steps
- Use indentation for sub-items
- Add headers for different sections
- Use → for outcomes or results
- Keep formatting minimal but clear

SMART ORGANIZATION:
- Group related items together
- Extract action items separately
- Identify key points or decisions
- Preserve priority indicators (urgent, important, ASAP)
- Detect and label sections appropriately

Examples:
"need milk bread eggs and also coffee" → 
"Shopping List:
• Milk
• Bread  
• Eggs
• Coffee"

"first set up the environment then install dependencies run tests and finally deploy" →
"Deployment Steps:
1. Set up the environment
2. Install dependencies
3. Run tests
4. Deploy"

"discussed the new feature timeline is two weeks john will handle backend sarah does frontend need to check budget" →
"Meeting Notes:
• New feature discussion
• Timeline: 2 weeks
• Assignments:
  - John: Backend
  - Sarah: Frontend
• Action: Check budget"

"ideas for app monetization could do subscriptions maybe ads or one time purchase also consider freemium model" →
"Monetization Ideas:
• Subscription model
• Advertisement integration
• One-time purchase
• Freemium approach"

"urgent fix the payment bug then update documentation tomorrow client meeting at 2pm prepare demo" →
"Tasks:
• URGENT: Fix payment bug
• Update documentation
• Tomorrow: Client meeting @ 2 PM
  - Prepare demo"

"project goals increase performance by 50 percent add multi language support launch by end of quarter" →
"Project Goals:
• Performance: Increase by 50%
• Feature: Multi-language support
• Launch: End of quarter"

Return ONLY the formatted notes."#;
