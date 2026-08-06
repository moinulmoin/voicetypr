use std::time::Instant;

use active_win_pos_rs::get_active_window;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::ai::error::{user_facing_message, AiProviderError};
use crate::ai::prompts::EnhancementPreset;
use crate::commands::settings::{
    normalize_final_text_language, normalize_transcription_task,
    FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
};
use crate::transcription::TranscriptionResult;
use crate::whisper::languages::validate_language;

use super::{
    apply_final_restoration_guard, apply_library_rules, apply_voice_command_stage, category_label,
    category_prompt_hint, classify, compile_context_for_target, load_writing_settings,
    sanitize_transcript, AppCategory, AppFormattingRule, AppliedWritingOperation, ContextHint,
    ProviderContextTarget, WritingError, WritingOperationKind, WritingProfile, WritingResult,
    WritingSettings, WritingStageTimings, WritingWarning,
};

fn enabled_app_rules(settings: &WritingSettings) -> impl Iterator<Item = &AppFormattingRule> {
    settings
        .app_formatting_rules
        .iter()
        .filter(|rule| rule.enabled && !rule.app_name.trim().is_empty())
}

fn app_rules_need_active_app(settings: &WritingSettings) -> bool {
    enabled_app_rules(settings).next().is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineAiState {
    pub stored_ai_enabled: bool,
    pub has_model_and_key: bool,
}

impl PipelineAiState {
    #[cfg(test)]
    fn from_validated_ai_enabled(ai_enabled: bool) -> Self {
        Self {
            stored_ai_enabled: ai_enabled,
            has_model_and_key: true,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub preset: EnhancementPreset,
    pub final_text_language: String,
    pub ai_effective: bool,
    pub category_hint: Option<AppCategory>,
}

struct PipelineConfigInputs {
    global_preset: EnhancementPreset,
    final_text_language: String,
    ai_state: PipelineAiState,
}

fn matching_app_formatting_preset(
    settings: &WritingSettings,
    active_app: Option<&ContextHint>,
) -> Option<EnhancementPreset> {
    let app_name = active_app?.app_name.as_deref()?.trim();
    if app_name.is_empty() {
        return None;
    }

    let normalized_app_name = app_name.to_ascii_lowercase();
    let matched_rule = enabled_app_rules(settings).find(|rule| {
        let rule_app_name = rule.app_name.trim().to_ascii_lowercase();
        normalized_app_name.contains(&rule_app_name)
    })?;

    Some(matched_rule.preset)
}

pub fn resolve_pipeline_config(
    settings: &WritingSettings,
    global_preset: EnhancementPreset,
    final_text_language: impl Into<String>,
    active_app: Option<&ContextHint>,
    ai_state: PipelineAiState,
) -> EffectiveConfig {
    let ai_effective = ai_state.stored_ai_enabled && ai_state.has_model_and_key;
    let matched_rule = matching_app_formatting_preset(settings, active_app);
    // Explicit per-app rule wins: suppress the auto category nudge based on
    // the raw rule match, not the AI-filtered preset, so a matching rule still
    // wins (and suppresses the nudge) even when AI is unavailable.
    let category_hint = if matched_rule.is_some() {
        None
    } else {
        active_app.map(classify)
    };
    let app_preset = matched_rule.filter(|preset| ai_effective || !preset.requires_ai_formatting());
    let mut preset = app_preset.unwrap_or(global_preset);

    if !ai_effective && preset.requires_ai_formatting() {
        preset = EnhancementPreset::PersonalDictation;
    }

    let final_text_language = if preset == EnhancementPreset::PersonalDictation {
        FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string()
    } else {
        final_text_language.into()
    };
    EffectiveConfig {
        preset,
        final_text_language,
        ai_effective,
        category_hint,
    }
}

fn read_pipeline_config_inputs(
    app: &AppHandle,
    ai_enabled: bool,
) -> Result<PipelineConfigInputs, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let global_preset = store
        .get("enhancement_options")
        .map(|value| {
            crate::ai::prompts::parse_enhancement_options_from_value(&value, ai_enabled)
                .map(|options| options.preset)
        })
        .transpose()?
        .unwrap_or_else(|| {
            crate::ai::EnhancementOptions::default_for_ai_enabled(ai_enabled).preset
        });
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
    let final_text_language =
        normalize_final_text_language(stored_final_text_language.as_deref(), &transcription_task);
    let ai_state = PipelineAiState {
        stored_ai_enabled: ai_enabled,
        has_model_and_key: crate::commands::ai::has_ai_model_and_key(app)?,
    };

    Ok(PipelineConfigInputs {
        global_preset,
        final_text_language,
        ai_state,
    })
}

pub fn effective_pipeline_config(
    app: &AppHandle,
    ai_enabled: bool,
) -> Result<EffectiveConfig, String> {
    let settings = load_writing_settings(app)?;
    let should_capture_active_app = app_rules_need_active_app(&settings) || ai_enabled;
    let active_app = capture_active_app_context(should_capture_active_app);
    let inputs = read_pipeline_config_inputs(app, ai_enabled)?;

    Ok(resolve_pipeline_config(
        &settings,
        inputs.global_preset,
        inputs.final_text_language,
        active_app.as_ref(),
        inputs.ai_state,
    ))
}

/// Resolves whether the effective writing mode is Personal Dictation for the current
/// foreground app, using the same preset resolution as `process_transcription`.
pub fn effective_personal_dictation_mode(
    app: &AppHandle,
    ai_enabled: bool,
) -> Result<bool, String> {
    Ok(effective_pipeline_config(app, ai_enabled)?.preset == EnhancementPreset::PersonalDictation)
}

async fn load_writing_profile(
    app: &AppHandle,
    ai_enabled: bool,
    settings: &WritingSettings,
    active_app: Option<&ContextHint>,
) -> Result<EffectiveConfig, String> {
    let inputs = read_pipeline_config_inputs(app, ai_enabled)?;

    Ok(resolve_pipeline_config(
        settings,
        inputs.global_preset,
        inputs.final_text_language,
        active_app,
        inputs.ai_state,
    ))
}

pub(crate) fn normalize_language_scope(value: Option<&str>) -> Option<String> {
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

pub(crate) fn language_scope_matches(
    scope: Option<&str>,
    transcript_language: Option<&str>,
) -> bool {
    match scope {
        Some(scope) => transcript_language == Some(scope),
        None => true,
    }
}

fn capture_active_app_context(should_capture: bool) -> Option<ContextHint> {
    if !should_capture {
        return None;
    }

    let window = get_active_window().ok()?;
    if window.app_name.trim().is_empty() {
        return None;
    }

    Some(ContextHint {
        app_name: Some(window.app_name),
        window_title: if window.title.trim().is_empty() {
            None
        } else {
            Some(window.title)
        },
        process_path: Some(window.process_path.to_string_lossy().to_string()),
        ..Default::default()
    })
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

    // The text was NOT transformed, so the output stays in the transcript's own
    // language — never the requested target. When the transcript language is
    // known, report it; when the source language is unknown (engine omitted
    // it, remote STT, pre-detection models), fall back to the "same as
    // transcript" sentinel rather than falsely reporting a target language the
    // transform never produced.
    *output_language = match transcript_language {
        Some(language) => language.to_string(),
        None => FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT.to_string(),
    };
}

pub(crate) fn smart_formatting_ai_context(
    settings: &WritingSettings,
    transcript_language: Option<&str>,
) -> Option<String> {
    compile_context_for_target(
        settings,
        transcript_language,
        ProviderContextTarget::SmartFormatting,
    )
}

struct SmartFormattingRequest<'a> {
    app: AppHandle,
    text: &'a str,
    transcript_language: Option<String>,
    output_language: &'a mut String,
    config: &'a EffectiveConfig,
    settings: &'a WritingSettings,
    needs_output_language_transform: bool,
    applied_operations: &'a mut Vec<AppliedWritingOperation>,
    warnings: &'a mut Vec<WritingWarning>,
}

async fn run_smart_formatting(
    request: SmartFormattingRequest<'_>,
) -> Result<(String, u64), AiProviderError> {
    let options = crate::ai::EnhancementOptions {
        preset: request.config.preset,
    };
    let ai_context =
        smart_formatting_ai_context(request.settings, request.transcript_language.as_deref());
    // Compose the behavioral nudge sentence for the resolved app category.
    // EffectiveConfig.category_hint is None when an explicit per-app rule
    // matched (user rule wins); Browser/Other carry no nudge, so we skip the
    // sentence entirely instead of injecting a context-only preamble.
    let app_category_hint: Option<String> = request.config.category_hint.and_then(|cat| {
        category_prompt_hint(cat).map(|hint| {
            format!(
                "You are dictating into a {} context. {}",
                category_label(cat),
                hint
            )
        })
    });
    match crate::commands::ai::polish_text_typed(
        &request.app,
        request.text,
        &options,
        Some(request.output_language.as_str()),
        request.transcript_language.as_deref(),
        ai_context.as_deref(),
        app_category_hint.as_deref(),
    )
    .await
    {
        Ok(result) => {
            let enhanced = result.output_text;
            if enhanced.trim().is_empty() {
                return Err(AiProviderError::BadResponse);
            }

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
                            request.output_language, request.config.preset
                        )
                    } else {
                        format!("Applied {:?} cleanup", request.config.preset)
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

            Ok((enhanced, result.duration_ms))
        }
        Err(error) => Err(error),
    }
}

fn resolve_smart_formatting_outcome(
    result: Result<(String, u64), AiProviderError>,
    library_text: &str,
    needs_output_language_transform: bool,
    _transcript_language: Option<&str>,
    output_language: &str,
    warnings: &mut Vec<WritingWarning>,
) -> Result<(String, Option<AiProviderError>, Option<u64>), WritingError> {
    match result {
        Ok((text, ai_polish_ms)) => Ok((text, None, Some(ai_polish_ms))),
        Err(error) if needs_output_language_transform => Err(WritingError::TranslationFailed {
            target_language: output_language.to_string(),
            detail: user_facing_message(&error).to_string(),
        }),
        Err(error) => {
            warnings.push(WritingWarning {
                code: "ai_formatting_failed".to_string(),
                message: format!(
                    "AI formatting failed ({}); used deterministic text instead",
                    user_facing_message(&error)
                ),
            });

            Ok((library_text.to_string(), Some(error), None))
        }
    }
}

pub async fn process_transcription(
    app: AppHandle,
    transcription: TranscriptionResult,
    ai_enabled: bool,
) -> Result<WritingResult, WritingError> {
    let settings = load_writing_settings(&app).map_err(WritingError::Config)?;
    let should_capture_active_app = app_rules_need_active_app(&settings) || ai_enabled;
    let mut active_app = capture_active_app_context(should_capture_active_app);
    let pipeline_config = load_writing_profile(&app, ai_enabled, &settings, active_app.as_ref())
        .await
        .map_err(WritingError::Config)?;
    // Stamp resolved category for transparency (history badge).
    if let Some(hint) = &mut active_app {
        hint.category = pipeline_config.category_hint;
    }
    let transcript_language = transcription.transcript_language.clone().or_else(|| {
        transcription
            .task
            .fallback_transcript_language(transcription.spoken_language.as_deref())
    });
    let output_profile = WritingProfile {
        mode: pipeline_config.preset,
        final_text_language: pipeline_config.final_text_language.clone(),
    };
    let mut output_language = resolve_output_language(&output_profile, &transcription);
    let mut applied_operations = Vec::new();
    let mut warnings = Vec::new();
    let deterministic_start = Instant::now();
    let cleaned_text = sanitize_transcript(&transcription.raw_text);
    if cleaned_text.as_ref() != transcription.raw_text {
        applied_operations.push(AppliedWritingOperation {
            kind: WritingOperationKind::TranscriptCleanup,
            detail: "Applied transcript cleanup".to_string(),
        });
    }
    let mut library_result = apply_library_rules(
        cleaned_text.as_ref(),
        &settings,
        transcript_language.as_deref(),
        &mut applied_operations,
    );
    apply_voice_command_stage(
        &mut library_result,
        &settings,
        transcript_language.as_deref(),
        &mut applied_operations,
    );
    let deterministic_ms = deterministic_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    log::info!(
        "transcription_stage_timing stage=deterministic duration_ms={}",
        deterministic_ms
    );

    // When the transcript language is known, a transform is needed iff it differs
    // from the configured output language. When it is unknown (engine omitted it,
    // remote STT, pre-detection models) we cannot confirm the transcript already
    // matches the target, so honor "fail truthfully": treat a transform as needed
    // when the user configured an explicit output language (triggering the
    // OutputLanguageRequiresAi guard or an AI run). In "same as transcript" mode
    // the user has not requested a language change, so an unknown source language
    // is left untouched (pass-through) rather than forcing a spurious error.
    let needs_output_language_transform = match transcript_language.as_deref() {
        Some(language) => language != output_language,
        None => pipeline_config.final_text_language != FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
    };

    let can_run_ai_formatting = pipeline_config.ai_effective
        && pipeline_config.preset != EnhancementPreset::PersonalDictation;

    if needs_output_language_transform && !can_run_ai_formatting && !library_result.literal_locked {
        return Err(WritingError::OutputLanguageRequiresAi);
    }

    if pipeline_config.preset.requires_ai_formatting()
        && !pipeline_config.ai_effective
        && !library_result.literal_locked
    {
        return Err(WritingError::Config(
            "This writing mode requires AI formatting. Enable AI formatting in settings or switch to Personal Dictation.".into(),
        ));
    }

    let should_run_ai = can_run_ai_formatting && !library_result.literal_locked;

    let mut ai_error = None;
    let mut ai_polish_ms = None;
    let mut final_text = if library_result.literal_locked {
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
    } else if should_run_ai {
        let (text, error, duration_ms) = resolve_smart_formatting_outcome(
            run_smart_formatting(SmartFormattingRequest {
                app,
                text: &library_result.text,
                transcript_language: transcript_language.clone(),
                output_language: &mut output_language,
                config: &pipeline_config,
                settings: &settings,
                needs_output_language_transform,
                applied_operations: &mut applied_operations,
                warnings: &mut warnings,
            })
            .await,
            &library_result.text,
            needs_output_language_transform,
            transcript_language.as_deref(),
            &output_language,
            &mut warnings,
        )?;
        ai_error = error;
        ai_polish_ms = duration_ms;
        if let Some(duration_ms) = ai_polish_ms {
            log::info!(
                "transcription_stage_timing stage=ai_polish duration_ms={}",
                duration_ms
            );
        }
        text
    } else {
        library_result.text.clone()
    };

    let (guarded_text, guard_operations) = apply_final_restoration_guard(
        &final_text,
        &library_result.provenance,
        library_result.literal_locked,
        needs_output_language_transform,
    );
    final_text = guarded_text;
    applied_operations.extend(guard_operations);

    Ok(WritingResult {
        raw_text: transcription.raw_text.clone(),
        ai_applied: should_run_ai && ai_error.is_none() && final_text != library_result.text,
        final_text,
        output_language,
        mode: pipeline_config.preset,
        applied_operations,
        warnings,
        context_hint: active_app,
        stage_timings: WritingStageTimings {
            deterministic_ms,
            ai_polish_ms,
            insertion_ms: None,
        },
        ai_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::prompts::EnhancementPreset;
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
    fn test_resolve_smart_formatting_outcome_preserves_success() {
        let mut warnings = Vec::new();
        let output_language = "en".to_string();
        let (out, error, duration_ms) = resolve_smart_formatting_outcome(
            Ok(("formatted".to_string(), 123)),
            "library",
            false,
            None,
            &output_language,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(out, "formatted");
        assert_eq!(error, None);
        assert_eq!(duration_ms, Some(123));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_resolve_smart_formatting_outcome_falls_back_when_translation_required() {
        let mut warnings = Vec::new();
        let output_language = "fr".to_string();
        let error = resolve_smart_formatting_outcome(
            Err(AiProviderError::Network),
            "library",
            true,
            Some("en"),
            &output_language,
            &mut warnings,
        )
        .unwrap_err();

        match error {
            WritingError::TranslationFailed {
                target_language,
                detail,
            } => {
                assert_eq!(target_language, "fr");
                assert_eq!(detail, "network error");
            }
            WritingError::OutputLanguageRequiresAi | WritingError::Config(_) => {
                panic!("expected translation failure")
            }
        }
        assert_eq!(output_language, "fr");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_resolve_smart_formatting_outcome_falls_back_without_translation() {
        let mut warnings = Vec::new();
        let output_language = "en".to_string();
        let (out, error, duration_ms) = resolve_smart_formatting_outcome(
            Err(AiProviderError::Timeout),
            "library text",
            false,
            None,
            &output_language,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(out, "library text");
        assert_eq!(error, Some(AiProviderError::Timeout));
        assert_eq!(duration_ms, None);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "ai_formatting_failed");
        assert!(warnings[0].message.contains("timed out"));
    }

    #[test]
    fn test_app_rule_message_overrides_global_personal_for_effective_mode() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };
        let global_preset = EnhancementPreset::PersonalDictation;
        let effective = resolve_pipeline_config(
            &settings,
            global_preset,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
            Some(&active_app),
            PipelineAiState::from_validated_ai_enabled(true),
        );

        assert_eq!(effective.preset, EnhancementPreset::Message);
        assert!(effective.preset.requires_ai_formatting());
    }

    #[test]
    fn test_resolve_output_language_prefers_transcript_language() {
        let profile = WritingProfile {
            mode: EnhancementPreset::CleanDictation,
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
    fn test_output_language_transform_fallback_unknown_transcript_language() {
        // Providers that omit transcript language still set an explicit target,
        // so `needs_output_language_transform` is true. When the transform does
        // not happen (AI returned unchanged text, or a snippet was preserved
        // literally) the requested target must NOT be reported: the output is
        // still in the (unknown) transcript language, represented by the
        // "same as transcript" sentinel. Regression for history/CLI reporting
        // the wrong language.
        let mut warnings = Vec::new();
        let mut output_language = "fr".to_string();

        record_output_language_transform_fallback(
            &mut warnings,
            &mut output_language,
            None,
            "snippet_literal_preserved",
            "Snippet preserved literally; output language was not transformed".to_string(),
        );

        assert_eq!(output_language, FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "snippet_literal_preserved");
    }

    #[test]
    fn test_resolve_output_language_falls_back_to_task_language() {
        let profile = WritingProfile {
            mode: EnhancementPreset::CleanDictation,
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
    fn test_app_formatting_rules_match_first_enabled_rule_and_skip_ai_when_disabled() {
        let settings = WritingSettings {
            app_formatting_rules: vec![
                AppFormattingRule {
                    app_name: "slack".to_string(),
                    preset: EnhancementPreset::Message,
                    enabled: true,
                },
                AppFormattingRule {
                    app_name: "slack".to_string(),
                    preset: EnhancementPreset::PersonalDictation,
                    enabled: true,
                },
            ],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::PersonalDictation,
                FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::Message
        );
        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::Message,
                FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(false),
            )
            .preset,
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_app_formatting_rules_use_case_insensitive_substring_match() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "cursor".to_string(),
                preset: EnhancementPreset::Code,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Cursor IDE".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::PersonalDictation,
                FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::Code
        );
    }

    #[test]
    fn test_app_formatting_rules_skip_disabled_rules() {
        let settings = WritingSettings {
            app_formatting_rules: vec![
                AppFormattingRule {
                    app_name: "slack".to_string(),
                    preset: EnhancementPreset::Message,
                    enabled: false,
                },
                AppFormattingRule {
                    app_name: "mail".to_string(),
                    preset: EnhancementPreset::Writing,
                    enabled: true,
                },
            ],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::CleanDictation,
                FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::CleanDictation
        );
    }

    #[test]
    fn test_resolve_pipeline_config_ai_disabled_is_personal_dictation() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::Message,
                "fr",
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(false),
            )
            .preset,
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_resolve_pipeline_config_app_rule_personal_dictation() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "notes".to_string(),
                preset: EnhancementPreset::PersonalDictation,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Apple Notes".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::Message,
                "fr",
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_resolve_pipeline_config_app_rule_message_overrides_global_personal() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::PersonalDictation,
                "fr",
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::Message
        );
        assert_ne!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::PersonalDictation,
                "fr",
                Some(&active_app),
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::PersonalDictation
        );
    }

    #[test]
    fn test_resolve_pipeline_config_falls_back_to_global_without_active_app() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };

        assert_eq!(
            resolve_pipeline_config(
                &settings,
                EnhancementPreset::PersonalDictation,
                "fr",
                None,
                PipelineAiState::from_validated_ai_enabled(true),
            )
            .preset,
            EnhancementPreset::PersonalDictation
        );
    }
    #[test]
    fn test_category_hint_none_when_explicit_app_rule_matches() {
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };
        let effective = resolve_pipeline_config(
            &settings,
            EnhancementPreset::PersonalDictation,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
            Some(&active_app),
            PipelineAiState::from_validated_ai_enabled(true),
        );
        assert_eq!(effective.preset, EnhancementPreset::Message);
        assert_eq!(
            effective.category_hint, None,
            "explicit rule must suppress category hint"
        );
    }

    #[test]
    fn test_category_hint_none_when_explicit_rule_matches_without_ai() {
        // FIX 1 regression: suppression must key off the raw rule match, not
        // the AI-filtered preset. With AI unavailable the rule's preset is
        // filtered out, but an explicit rule still suppresses the nudge.
        let settings = WritingSettings {
            app_formatting_rules: vec![AppFormattingRule {
                app_name: "slack".to_string(),
                preset: EnhancementPreset::Message,
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };
        let effective = resolve_pipeline_config(
            &settings,
            EnhancementPreset::PersonalDictation,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
            Some(&active_app),
            PipelineAiState::from_validated_ai_enabled(false),
        );
        assert!(
            !effective.ai_effective,
            "AI must be ineffective in this scenario"
        );
        assert_eq!(
            effective.preset,
            EnhancementPreset::PersonalDictation,
            "AI-requiring rule preset falls back to global when AI unavailable"
        );
        assert_eq!(
            effective.category_hint, None,
            "explicit rule must suppress category hint even when AI unavailable"
        );
    }

    #[test]
    fn test_category_hint_some_when_no_explicit_rule_matches() {
        let settings = WritingSettings::default();
        let active_app = ContextHint {
            app_name: Some("Slack Desktop".to_string()),
            ..Default::default()
        };
        let effective = resolve_pipeline_config(
            &settings,
            EnhancementPreset::CleanDictation,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
            Some(&active_app),
            PipelineAiState::from_validated_ai_enabled(true),
        );
        assert_eq!(effective.preset, EnhancementPreset::CleanDictation);
        assert_eq!(
            effective.category_hint,
            Some(crate::writing::AppCategory::Chat)
        );
    }

    #[test]
    fn test_category_hint_none_when_no_active_app() {
        let settings = WritingSettings::default();
        let effective = resolve_pipeline_config(
            &settings,
            EnhancementPreset::CleanDictation,
            FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT,
            None,
            PipelineAiState::from_validated_ai_enabled(true),
        );
        assert_eq!(effective.category_hint, None);
    }
}
