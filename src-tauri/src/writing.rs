use active_win_pos_rs::get_active_window;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::ai::prompts::EnhancementPreset;
use crate::commands::settings::{
    normalize_final_text_language, normalize_transcription_task,
    FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
};
use crate::transcription::TranscriptionResult;
use crate::whisper::languages::validate_language;

const WRITING_SETTINGS_KEY: &str = "writing_settings";

fn default_enabled() -> bool {
    true
}

fn default_preserve_literal() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingMode {
    CleanDictation,
    Writing,
    Notes,
    Message,
    Coding,
}

impl From<EnhancementPreset> for WritingMode {
    fn from(value: EnhancementPreset) -> Self {
        match value {
            EnhancementPreset::Default => Self::CleanDictation,
            EnhancementPreset::Writing => Self::Writing,
            EnhancementPreset::Notes => Self::Notes,
            EnhancementPreset::Message => Self::Message,
            EnhancementPreset::Coding => Self::Coding,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextPolicy {
    #[default]
    Off,
    AppHintOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TextReplacementRule {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomWord {
    pub phrase: String,
    #[serde(default)]
    pub spoken_form: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Snippet {
    pub trigger: String,
    pub body: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_preserve_literal")]
    pub preserve_literal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WritingSettings {
    #[serde(default)]
    pub replacements: Vec<TextReplacementRule>,
    #[serde(default)]
    pub custom_words: Vec<CustomWord>,
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    #[serde(default)]
    pub context_policy: ContextPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritingProfile {
    pub mode: WritingMode,
    pub final_text_language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingOperationKind {
    Replacement,
    Snippet,
    Translation,
    AiCleanup,
    ContextHint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedWritingOperation {
    pub kind: WritingOperationKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritingWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextHint {
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub app_category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WritingResult {
    pub raw_text: String,
    pub final_text: String,
    pub output_language: String,
    pub mode: WritingMode,
    pub ai_applied: bool,
    #[serde(default)]
    pub applied_operations: Vec<AppliedWritingOperation>,
    #[serde(default)]
    pub warnings: Vec<WritingWarning>,
    #[serde(default)]
    pub context_hint: Option<ContextHint>,
}

pub fn sanitize_writing_settings(settings: WritingSettings) -> WritingSettings {
    WritingSettings {
        replacements: settings
            .replacements
            .into_iter()
            .filter_map(|rule| {
                let from = rule.from.trim();
                let to = rule.to.trim();
                if from.is_empty() || to.is_empty() {
                    return None;
                }
                Some(TextReplacementRule {
                    from: from.to_string(),
                    to: to.to_string(),
                    language: normalize_language_scope(rule.language.as_deref()),
                    enabled: rule.enabled,
                })
            })
            .collect(),
        custom_words: settings
            .custom_words
            .into_iter()
            .filter_map(|word| {
                let phrase = word.phrase.trim();
                if phrase.is_empty() {
                    return None;
                }
                let spoken_form = word.spoken_form.and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                });
                Some(CustomWord {
                    phrase: phrase.to_string(),
                    spoken_form,
                    language: normalize_language_scope(word.language.as_deref()),
                    enabled: word.enabled,
                })
            })
            .collect(),
        snippets: settings
            .snippets
            .into_iter()
            .filter_map(|snippet| {
                let trigger = snippet.trigger.trim();
                let body = snippet.body.trim_end();
                if trigger.is_empty() || body.is_empty() {
                    return None;
                }
                Some(Snippet {
                    trigger: trigger.to_string(),
                    body: body.to_string(),
                    language: normalize_language_scope(snippet.language.as_deref()),
                    enabled: snippet.enabled,
                    preserve_literal: snippet.preserve_literal,
                })
            })
            .collect(),
        context_policy: settings.context_policy,
    }
}

pub fn load_writing_settings(app: &AppHandle) -> Result<WritingSettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    if let Some(value) = store.get(WRITING_SETTINGS_KEY) {
        let settings: WritingSettings = serde_json::from_value(value.clone())
            .map_err(|e| format!("Failed to parse writing settings: {}", e))?;
        Ok(sanitize_writing_settings(settings))
    } else {
        Ok(WritingSettings::default())
    }
}

pub fn save_writing_settings(app: &AppHandle, settings: &WritingSettings) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let sanitized = sanitize_writing_settings(settings.clone());
    store.set(
        WRITING_SETTINGS_KEY,
        serde_json::to_value(&sanitized)
            .map_err(|e| format!("Failed to serialize writing settings: {}", e))?,
    );
    store
        .save()
        .map_err(|e| format!("Failed to save writing settings: {}", e))
}

async fn load_writing_profile(app: &AppHandle) -> Result<WritingProfile, String> {
    let options = crate::commands::ai::get_enhancement_options(app.clone())
        .await
        .unwrap_or_default();
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = normalize_transcription_task(
        stored_transcription_task.as_deref(),
        legacy_translate_to_english,
    );
    let stored_final_text_language = store
        .get("final_text_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(WritingProfile {
        mode: options.preset.into(),
        final_text_language: normalize_final_text_language(
            stored_final_text_language.as_deref(),
            &transcription_task,
        ),
    })
}

fn normalize_language_scope(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| validate_language(Some(trimmed)).to_string())
    })
}

fn resolve_output_language(
    profile: &WritingProfile,
    transcription: &TranscriptionResult,
) -> String {
    if profile.final_text_language == FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT {
        transcription
            .transcript_language
            .clone()
            .or_else(|| {
                transcription
                    .task
                    .fallback_transcript_language(transcription.spoken_language.as_deref())
            })
            .unwrap_or_else(|| "en".to_string())
    } else {
        profile.final_text_language.clone()
    }
}

fn language_scope_matches(scope: Option<&str>, transcript_language: Option<&str>) -> bool {
    match scope {
        Some(scope) => transcript_language == Some(scope),
        None => true,
    }
}

fn candidate_has_boundaries(text: &str, start: usize, end: usize) -> bool {
    let left_ok = text[..start]
        .chars()
        .last()
        .map(|ch| !ch.is_alphanumeric())
        .unwrap_or(true);
    let right_ok = text[end..]
        .chars()
        .next()
        .map(|ch| !ch.is_alphanumeric())
        .unwrap_or(true);
    left_ok && right_ok
}

#[derive(Clone)]
struct ReplacementCandidate {
    start: usize,
    end: usize,
    replacement: String,
    detail: String,
    priority: u8,
}

fn collect_replacement_candidates(
    text: &str,
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
) -> Vec<ReplacementCandidate> {
    let mut candidates = Vec::new();

    for rule in replacements.iter().filter(|rule| {
        rule.enabled && language_scope_matches(rule.language.as_deref(), transcript_language)
    }) {
        let Ok(regex) = RegexBuilder::new(&regex::escape(&rule.from))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };

        for mat in regex.find_iter(text) {
            if !candidate_has_boundaries(text, mat.start(), mat.end()) {
                continue;
            }
            candidates.push(ReplacementCandidate {
                start: mat.start(),
                end: mat.end(),
                replacement: rule.to.clone(),
                detail: format!("{} → {}", rule.from, rule.to),
                priority: 2,
            });
        }
    }

    for word in custom_words.iter().filter(|word| {
        word.enabled && language_scope_matches(word.language.as_deref(), transcript_language)
    }) {
        let Some(spoken_form) = word.spoken_form.as_deref() else {
            continue;
        };
        let Ok(regex) = RegexBuilder::new(&regex::escape(spoken_form))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };

        for mat in regex.find_iter(text) {
            if !candidate_has_boundaries(text, mat.start(), mat.end()) {
                continue;
            }
            candidates.push(ReplacementCandidate {
                start: mat.start(),
                end: mat.end(),
                replacement: word.phrase.clone(),
                detail: format!("{} → {}", spoken_form, word.phrase),
                priority: 1,
            });
        }
    }

    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then(right.priority.cmp(&left.priority))
            .then((right.end - right.start).cmp(&(left.end - left.start)))
    });
    candidates
}

fn apply_text_replacements(
    text: &str,
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
) -> (String, Vec<AppliedWritingOperation>) {
    let candidates =
        collect_replacement_candidates(text, replacements, custom_words, transcript_language);
    if candidates.is_empty() {
        return (text.to_string(), Vec::new());
    }

    let mut selected = Vec::new();
    let mut cursor = 0usize;
    for candidate in candidates {
        if candidate.start < cursor {
            continue;
        }
        cursor = candidate.end;
        selected.push(candidate);
    }

    if selected.is_empty() {
        return (text.to_string(), Vec::new());
    }

    let mut output = String::with_capacity(text.len());
    let mut operations = Vec::with_capacity(selected.len());
    let mut last = 0usize;

    for candidate in selected {
        output.push_str(&text[last..candidate.start]);
        output.push_str(&candidate.replacement);
        operations.push(AppliedWritingOperation {
            kind: WritingOperationKind::Replacement,
            detail: candidate.detail,
        });
        last = candidate.end;
    }
    output.push_str(&text[last..]);

    (output, operations)
}

fn match_snippet<'a>(
    text: &str,
    snippets: &'a [Snippet],
    transcript_language: Option<&str>,
) -> Option<&'a Snippet> {
    let trimmed = text.trim();
    snippets
        .iter()
        .filter(|snippet| {
            snippet.enabled
                && language_scope_matches(snippet.language.as_deref(), transcript_language)
        })
        .filter(|snippet| snippet.trigger.eq_ignore_ascii_case(trimmed))
        .max_by_key(|snippet| snippet.trigger.len())
}

fn classify_app_category(app_name: &str) -> Option<String> {
    let normalized = app_name.to_ascii_lowercase();
    let category = if ["mail", "outlook", "spark"]
        .iter()
        .any(|value| normalized.contains(value))
    {
        Some("email")
    } else if [
        "messages", "slack", "discord", "telegram", "signal", "teams",
    ]
    .iter()
    .any(|value| normalized.contains(value))
    {
        Some("chat")
    } else if [
        "code", "cursor", "xcode", "terminal", "iterm", "warp", "zed", "sublime",
    ]
    .iter()
    .any(|value| normalized.contains(value))
    {
        Some("editor")
    } else if ["notes", "obsidian", "notion", "bear"]
        .iter()
        .any(|value| normalized.contains(value))
    {
        Some("notes")
    } else {
        None
    };

    category.map(str::to_string)
}

fn capture_context_hint(policy: ContextPolicy) -> Option<ContextHint> {
    if policy != ContextPolicy::AppHintOnly {
        return None;
    }

    let window = get_active_window().ok()?;
    if window.app_name.trim().is_empty() {
        return None;
    }

    Some(ContextHint {
        app_category: classify_app_category(&window.app_name),
        app_name: Some(window.app_name),
    })
}

fn build_ai_context(
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
    context_hint: Option<&ContextHint>,
) -> Option<String> {
    let preferred_terms: Vec<String> = custom_words
        .iter()
        .filter(|word| {
            word.enabled && language_scope_matches(word.language.as_deref(), transcript_language)
        })
        .map(|word| match word.spoken_form.as_deref() {
            Some(spoken_form) => format!("{} (spoken as {})", word.phrase, spoken_form),
            None => word.phrase.clone(),
        })
        .collect();

    let mut sections = Vec::new();
    if !preferred_terms.is_empty() {
        sections.push(format!(
            "Preferred terms: {}. Preserve exact spelling/casing.",
            preferred_terms.join(", ")
        ));
    }

    if let Some(context_hint) = context_hint {
        if context_hint.app_name.is_some() || context_hint.app_category.is_some() {
            let mut context_parts = Vec::new();
            if let Some(app_name) = context_hint.app_name.as_deref() {
                context_parts.push(format!("app {}", app_name));
            }
            if let Some(category) = context_hint.app_category.as_deref() {
                context_parts.push(format!("category {}", category));
            }
            sections.push(format!(
                "Context hint: target {}.",
                context_parts.join(", ")
            ));
        }
    }

    (!sections.is_empty()).then(|| sections.join(" "))
}

struct LibraryRulesResult {
    text: String,
    literal_locked: bool,
}

fn run_transcript_cleanup_mechanical(text: &str) -> &str {
    text
}

fn apply_library_rules(
    text: &str,
    settings: &WritingSettings,
    transcript_language: Option<&str>,
    applied_operations: &mut Vec<AppliedWritingOperation>,
) -> LibraryRulesResult {
    let snippet_match = match_snippet(text, &settings.snippets, transcript_language);

    if let Some(snippet) = snippet_match {
        applied_operations.push(AppliedWritingOperation {
            kind: WritingOperationKind::Snippet,
            detail: format!("{} → literal snippet", snippet.trigger),
        });
        return LibraryRulesResult {
            text: snippet.body.clone(),
            literal_locked: snippet.preserve_literal,
        };
    }

    let (replaced_text, replacement_ops) = apply_text_replacements(
        text,
        &settings.replacements,
        &settings.custom_words,
        transcript_language,
    );
    applied_operations.extend(replacement_ops);

    LibraryRulesResult {
        text: replaced_text,
        literal_locked: false,
    }
}

fn record_output_language_transform_fallback(
    warnings: &mut Vec<WritingWarning>,
    output_language: &mut String,
    transcript_language: Option<&str>,
    code: &str,
    message: String,
) {
    warnings.push(WritingWarning {
        code: code.to_string(),
        message,
    });

    if let Some(language) = transcript_language {
        *output_language = language.to_string();
    }
}

struct SmartFormattingRequest<'a> {
    app: AppHandle,
    text: &'a str,
    transcript_language: Option<String>,
    output_language: &'a mut String,
    profile: &'a WritingProfile,
    settings: &'a WritingSettings,
    context_hint: Option<&'a ContextHint>,
    needs_output_language_transform: bool,
    applied_operations: &'a mut Vec<AppliedWritingOperation>,
    warnings: &'a mut Vec<WritingWarning>,
}

async fn run_smart_formatting(request: SmartFormattingRequest<'_>) -> Result<String, String> {
    let ai_context = build_ai_context(
        &request.settings.custom_words,
        request.transcript_language.as_deref(),
        request.context_hint,
    );
    match crate::commands::ai::enhance_transcription(
        request.text.to_string(),
        request.transcript_language.clone(),
        Some(true),
        Some(request.output_language.clone()),
        ai_context,
        request.app,
    )
    .await
    {
        Ok(enhanced) => {
            if enhanced != request.text {
                request.applied_operations.push(AppliedWritingOperation {
                    kind: if request.needs_output_language_transform {
                        WritingOperationKind::Translation
                    } else {
                        WritingOperationKind::AiCleanup
                    },
                    detail: if request.needs_output_language_transform {
                        format!(
                            "Translated/rewrote transcript to {} using {:?}",
                            request.output_language, request.profile.mode
                        )
                    } else {
                        format!("Applied {:?} cleanup", request.profile.mode)
                    },
                });
            } else if request.needs_output_language_transform {
                record_output_language_transform_fallback(
                    request.warnings,
                    request.output_language,
                    request.transcript_language.as_deref(),
                    "output_language_transform_failed",
                    format!(
                        "AI formatting returned the original transcript; output language remains {}",
                        request
                            .transcript_language
                            .as_deref()
                            .unwrap_or("the transcript language")
                    ),
                );
            }

            if request.context_hint.is_some() {
                request.applied_operations.push(AppliedWritingOperation {
                    kind: WritingOperationKind::ContextHint,
                    detail: "Used active app context hint".to_string(),
                });
            }

            Ok(enhanced)
        }
        Err(error) if !request.needs_output_language_transform => {
            request.warnings.push(WritingWarning {
                code: "ai_cleanup_failed".to_string(),
                message: format!(
                    "AI cleanup failed; deterministic writing result was used: {}",
                    error
                ),
            });
            Ok(request.text.to_string())
        }
        Err(error) => Err(error),
    }
}

pub async fn process_transcription(
    app: AppHandle,
    transcription: TranscriptionResult,
    ai_enabled: bool,
) -> Result<WritingResult, String> {
    let profile = load_writing_profile(&app).await?;
    let settings = load_writing_settings(&app)?;
    let transcript_language = transcription.transcript_language.clone().or_else(|| {
        transcription
            .task
            .fallback_transcript_language(transcription.spoken_language.as_deref())
    });
    let mut output_language = resolve_output_language(&profile, &transcription);
    let context_hint = capture_context_hint(settings.context_policy);

    let mut applied_operations = Vec::new();
    let mut warnings = Vec::new();
    let cleaned_text = run_transcript_cleanup_mechanical(&transcription.raw_text);
    let library_result = apply_library_rules(
        cleaned_text,
        &settings,
        transcript_language.as_deref(),
        &mut applied_operations,
    );

    let needs_output_language_transform = transcript_language
        .as_deref()
        .map(|language| language != output_language)
        .unwrap_or(false);

    if needs_output_language_transform && !ai_enabled && !library_result.literal_locked {
        return Err(
            "Final output language requires AI enhancement or native translation".to_string(),
        );
    }

    let final_text = if library_result.literal_locked {
        if needs_output_language_transform {
            record_output_language_transform_fallback(
                &mut warnings,
                &mut output_language,
                transcript_language.as_deref(),
                "snippet_literal_preserved",
                "Snippet preserved literally; output language was not transformed".to_string(),
            );
        }
        library_result.text.clone()
    } else if ai_enabled {
        run_smart_formatting(SmartFormattingRequest {
            app,
            text: &library_result.text,
            transcript_language: transcript_language.clone(),
            output_language: &mut output_language,
            profile: &profile,
            settings: &settings,
            context_hint: context_hint.as_ref(),
            needs_output_language_transform,
            applied_operations: &mut applied_operations,
            warnings: &mut warnings,
        })
        .await?
    } else {
        library_result.text.clone()
    };

    Ok(WritingResult {
        raw_text: transcription.raw_text.clone(),
        ai_applied: ai_enabled && final_text != library_result.text,
        final_text,
        output_language,
        mode: profile.mode,
        applied_operations,
        warnings,
        context_hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::{TranscriptionJob, TranscriptionSource, TranscriptionTask};

    fn make_result(
        raw_text: &str,
        spoken_language: Option<&str>,
        transcript_language: Option<&str>,
        task: TranscriptionTask,
    ) -> TranscriptionResult {
        let job = TranscriptionJob {
            source: TranscriptionSource::DesktopRecording,
            engine: "whisper".to_string(),
            model: "base".to_string(),
            spoken_language: spoken_language.map(str::to_string),
            task,
        };
        TranscriptionResult::new(&job, raw_text.to_string())
            .with_transcript_language(transcript_language.map(str::to_string))
    }

    #[test]
    fn test_writing_mode_maps_from_presets() {
        assert_eq!(
            WritingMode::from(EnhancementPreset::Default),
            WritingMode::CleanDictation
        );
        assert_eq!(
            WritingMode::from(EnhancementPreset::Writing),
            WritingMode::Writing
        );
        assert_eq!(
            WritingMode::from(EnhancementPreset::Notes),
            WritingMode::Notes
        );
        assert_eq!(
            WritingMode::from(EnhancementPreset::Message),
            WritingMode::Message
        );
        assert_eq!(
            WritingMode::from(EnhancementPreset::Coding),
            WritingMode::Coding
        );
    }

    #[test]
    fn test_resolve_output_language_prefers_transcript_language() {
        let profile = WritingProfile {
            mode: WritingMode::CleanDictation,
            final_text_language: FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string(),
        };
        let transcription = make_result(
            "hola mundo",
            Some("es"),
            Some("es"),
            TranscriptionTask::Transcribe,
        );

        assert_eq!(resolve_output_language(&profile, &transcription), "es");
    }

    #[test]
    fn test_output_language_transform_fallback_restores_transcript_language() {
        let mut warnings = Vec::new();
        let mut output_language = "fr".to_string();

        record_output_language_transform_fallback(
            &mut warnings,
            &mut output_language,
            Some("es"),
            "output_language_transform_failed",
            "AI formatting returned original text".to_string(),
        );

        assert_eq!(output_language, "es");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "output_language_transform_failed");
    }

    #[test]
    fn test_resolve_output_language_falls_back_to_task_language() {
        let profile = WritingProfile {
            mode: WritingMode::CleanDictation,
            final_text_language: FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string(),
        };
        let transcription = make_result(
            "hello world",
            Some("es"),
            None,
            TranscriptionTask::TranslateToEnglish,
        );

        assert_eq!(resolve_output_language(&profile, &transcription), "en");
    }

    #[test]
    fn test_sanitize_writing_settings_trims_and_drops_empty_entries() {
        let settings = sanitize_writing_settings(WritingSettings {
            replacements: vec![
                TextReplacementRule {
                    from: " voice typer ".to_string(),
                    to: " VoiceTypr ".to_string(),
                    language: Some(" en ".to_string()),
                    enabled: true,
                },
                TextReplacementRule::default(),
            ],
            custom_words: vec![
                CustomWord {
                    phrase: " OpenAI ".to_string(),
                    spoken_form: Some(" open ai ".to_string()),
                    language: Some("en".to_string()),
                    enabled: true,
                },
                CustomWord::default(),
            ],
            snippets: vec![
                Snippet {
                    trigger: " insert note ".to_string(),
                    body: "Hello\n".to_string(),
                    language: Some("en".to_string()),
                    enabled: true,
                    preserve_literal: true,
                },
                Snippet::default(),
            ],
            context_policy: ContextPolicy::AppHintOnly,
        });

        assert_eq!(settings.replacements.len(), 1);
        assert_eq!(settings.replacements[0].from, "voice typer");
        assert_eq!(settings.replacements[0].to, "VoiceTypr");
        assert_eq!(settings.custom_words.len(), 1);
        assert_eq!(settings.custom_words[0].phrase, "OpenAI");
        assert_eq!(
            settings.custom_words[0].spoken_form.as_deref(),
            Some("open ai")
        );
        assert_eq!(settings.snippets.len(), 1);
        assert_eq!(settings.snippets[0].trigger, "insert note");
        assert_eq!(settings.snippets[0].body, "Hello");
    }

    #[test]
    fn test_apply_text_replacements_prefers_explicit_rules() {
        let replacements = vec![TextReplacementRule {
            from: "voice typer".to_string(),
            to: "VoiceTypr".to_string(),
            language: Some("en".to_string()),
            enabled: true,
        }];
        let custom_words = vec![CustomWord {
            phrase: "VoiceTypr".to_string(),
            spoken_form: Some("voice typer".to_string()),
            language: Some("en".to_string()),
            enabled: true,
        }];

        let (text, ops) = apply_text_replacements(
            "voice typer rules",
            &replacements,
            &custom_words,
            Some("en"),
        );

        assert_eq!(text, "VoiceTypr rules");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, WritingOperationKind::Replacement);
    }

    #[test]
    fn test_custom_word_spoken_form_creates_correction() {
        let (text, ops) = apply_text_replacements(
            "voice typer launched",
            &[],
            &[CustomWord {
                phrase: "VoiceTypr".to_string(),
                spoken_form: Some("voice typer".to_string()),
                language: Some("en".to_string()),
                enabled: true,
            }],
            Some("en"),
        );

        assert_eq!(text, "VoiceTypr launched");
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn test_snippet_match_is_whole_utterance_only() {
        let snippets = vec![Snippet {
            trigger: "insert note".to_string(),
            body: "Saved body".to_string(),
            language: None,
            enabled: true,
            preserve_literal: true,
        }];

        assert!(match_snippet("insert note", &snippets, Some("en")).is_some());
        assert!(match_snippet("please insert note", &snippets, Some("en")).is_none());
    }

    #[test]
    fn test_classify_app_category_maps_known_apps() {
        assert_eq!(classify_app_category("Mail"), Some("email".to_string()));
        assert_eq!(classify_app_category("Slack"), Some("chat".to_string()));
        assert_eq!(classify_app_category("Cursor"), Some("editor".to_string()));
        assert_eq!(classify_app_category("Obsidian"), Some("notes".to_string()));
        assert_eq!(classify_app_category("Unknown"), None);
    }

    #[test]
    fn test_build_ai_context_includes_terms_and_app_hint() {
        let context = build_ai_context(
            &[CustomWord {
                phrase: "VoiceTypr".to_string(),
                spoken_form: Some("voice typer".to_string()),
                language: Some("en".to_string()),
                enabled: true,
            }],
            Some("en"),
            Some(&ContextHint {
                app_name: Some("Mail".to_string()),
                app_category: Some("email".to_string()),
            }),
        )
        .unwrap();

        assert!(context.contains("VoiceTypr"));
        assert!(context.contains("voice typer"));
        assert!(context.contains("Mail"));
        assert!(context.contains("email"));
    }

    #[test]
    fn test_phrase_only_custom_word_does_not_replace_text() {
        let (text, ops) = apply_text_replacements(
            "shawn joined the call",
            &[],
            &[CustomWord {
                phrase: "Sean".to_string(),
                spoken_form: None,
                language: Some("en".to_string()),
                enabled: true,
            }],
            Some("en"),
        );

        assert_eq!(text, "shawn joined the call");
        assert!(ops.is_empty());
    }

    #[test]
    fn test_replacements_require_boundaries_and_language_match() {
        let replacements = vec![TextReplacementRule {
            from: "react".to_string(),
            to: "React".to_string(),
            language: Some("en".to_string()),
            enabled: true,
        }];

        let (identifier_text, identifier_ops) =
            apply_text_replacements("createReactiveStore", &replacements, &[], Some("en"));
        assert_eq!(identifier_text, "createReactiveStore");
        assert!(identifier_ops.is_empty());

        let (language_text, language_ops) =
            apply_text_replacements("react", &replacements, &[], Some("fr"));
        assert_eq!(language_text, "react");
        assert!(language_ops.is_empty());
    }

    #[test]
    fn test_build_ai_context_excludes_disabled_and_wrong_language_terms() {
        let context = build_ai_context(
            &[
                CustomWord {
                    phrase: "VoiceTypr".to_string(),
                    spoken_form: Some("voice typer".to_string()),
                    language: Some("en".to_string()),
                    enabled: true,
                },
                CustomWord {
                    phrase: "DisabledTerm".to_string(),
                    spoken_form: None,
                    language: Some("en".to_string()),
                    enabled: false,
                },
                CustomWord {
                    phrase: "TermeFrançais".to_string(),
                    spoken_form: None,
                    language: Some("fr".to_string()),
                    enabled: true,
                },
            ],
            Some("en"),
            None,
        )
        .unwrap();

        assert!(context.contains("VoiceTypr"));
        assert!(!context.contains("DisabledTerm"));
        assert!(!context.contains("TermeFrançais"));
    }
}
