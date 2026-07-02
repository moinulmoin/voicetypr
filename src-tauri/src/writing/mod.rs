mod library_rules;
mod pipeline;
mod settings;
mod vocabulary;

#[allow(unused_imports)]
pub use pipeline::{effective_personal_dictation_mode, process_transcription};
#[allow(unused_imports)]
pub use settings::{
    load_writing_settings, sanitize_writing_settings, save_writing_settings, AppFormattingRule,
    AppliedWritingOperation, ContextHint, CustomWord, Snippet, TextReplacementRule,
    VoiceCommandRule, WritingError, WritingMode, WritingOperationKind, WritingProfile,
    WritingResult, WritingSettings, WritingStageTimings, WritingWarning,
};
#[allow(unused_imports)]
pub use vocabulary::{
    compile_context_for_target, compile_deepgram_keyterms, compile_parakeet_custom_vocabulary,
    compile_soniox_context, ProviderContextCapabilities, ProviderContextTarget, SonioxContext,
    SonioxContextField,
};

#[allow(unused_imports)]
pub(crate) use library_rules::{
    apply_final_restoration_guard, apply_library_rules, apply_voice_command_stage,
    sanitize_transcript, voice_command_output,
};
#[allow(unused_imports)]
pub(crate) use pipeline::{
    language_scope_matches, normalize_language_scope, smart_formatting_ai_context,
};
