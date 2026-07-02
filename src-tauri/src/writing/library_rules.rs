use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use regex::{Regex, RegexBuilder};

use super::{
    language_scope_matches, normalize_language_scope, AppliedWritingOperation, CustomWord, Snippet,
    TextReplacementRule, VoiceCommandRule, WritingOperationKind, WritingSettings,
};

fn is_boundary_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn candidate_has_boundaries(text: &str, start: usize, end: usize) -> bool {
    let left_ok = text[..start]
        .chars()
        .last()
        .map(|ch| !is_boundary_word_char(ch))
        .unwrap_or(true);
    let right_ok = text[end..]
        .chars()
        .next()
        .map(|ch| !is_boundary_word_char(ch))
        .unwrap_or(true);
    left_ok && right_ok
}

fn span_in_protected_token(text: &str, start: usize, end: usize) -> bool {
    let token_start = text[..start]
        .char_indices()
        .rev()
        .find_map(|(index, ch)| ch.is_whitespace().then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let token_end = text[end..]
        .char_indices()
        .find_map(|(index, ch)| ch.is_whitespace().then_some(end + index))
        .unwrap_or(text.len());
    let token = &text[token_start..token_end];

    token.contains("://")
        || token
            .match_indices('@')
            .any(|(index, _)| index > 0 && index + 1 < token.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryRuleSourceKind {
    ExplicitReplacement,
    CustomWordSpokenForm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LibraryRuleApplication {
    source_form: String,
    target_text: String,
    source_kind: LibraryRuleSourceKind,
    language_scope: Option<String>,
    start: usize,
    end: usize,
    target_start: usize,
    target_end: usize,
}

#[derive(Clone)]
struct CompiledReplacementRule {
    regex: Regex,
    replacement: String,
    detail: String,
    priority: u8,
    source_form: String,
    source_kind: LibraryRuleSourceKind,
    language_scope: Option<String>,
}

#[derive(Clone)]
struct CompiledVoiceCommandRule {
    regex: Regex,
    phrase: String,
    output: VoiceCommandOutput,
    language_scope: Option<String>,
}

static REPLACEMENT_RULE_CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<CompiledReplacementRule>>>>> =
    OnceLock::new();
static VOICE_COMMAND_RULE_CACHE: OnceLock<
    Mutex<HashMap<String, Arc<Vec<CompiledVoiceCommandRule>>>>,
> = OnceLock::new();

fn push_fingerprint_field(fingerprint: &mut String, value: &str) {
    fingerprint.push_str(&value.len().to_string());
    fingerprint.push(':');
    fingerprint.push_str(value);
    fingerprint.push(';');
}

fn push_fingerprint_option(fingerprint: &mut String, value: Option<&str>) {
    match value {
        Some(value) => {
            fingerprint.push('S');
            push_fingerprint_field(fingerprint, value);
        }
        None => fingerprint.push_str("N;"),
    }
}

fn replacement_rules_fingerprint(
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
) -> String {
    let mut fingerprint = String::new();
    for rule in replacements.iter().filter(|rule| rule.enabled) {
        fingerprint.push_str("R;i;");
        push_fingerprint_field(&mut fingerprint, &rule.from);
        push_fingerprint_field(&mut fingerprint, &rule.to);
        push_fingerprint_option(&mut fingerprint, rule.language.as_deref());
    }
    for word in custom_words.iter().filter(|word| word.enabled) {
        let Some(spoken_form) = word.spoken_form.as_deref() else {
            continue;
        };
        fingerprint.push_str("C;i;");
        push_fingerprint_field(&mut fingerprint, spoken_form);
        push_fingerprint_field(&mut fingerprint, &word.phrase);
        push_fingerprint_option(&mut fingerprint, word.language.as_deref());
    }
    fingerprint
}

fn compiled_replacement_rules(
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
) -> Arc<Vec<CompiledReplacementRule>> {
    let fingerprint = replacement_rules_fingerprint(replacements, custom_words);
    let cache = REPLACEMENT_RULE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(compiled) = cache
        .lock()
        .expect("replacement rule cache poisoned")
        .get(&fingerprint)
    {
        return Arc::clone(compiled);
    }

    let mut compiled = Vec::new();
    for rule in replacements.iter().filter(|rule| rule.enabled) {
        let Ok(regex) = RegexBuilder::new(&regex::escape(&rule.from))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        compiled.push(CompiledReplacementRule {
            regex,
            replacement: rule.to.clone(),
            detail: format!("{} → {}", rule.from, rule.to),
            priority: 2,
            source_form: rule.from.clone(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: normalize_language_scope(rule.language.as_deref()),
        });
    }
    for word in custom_words.iter().filter(|word| word.enabled) {
        let Some(spoken_form) = word.spoken_form.as_deref() else {
            continue;
        };
        let Ok(regex) = RegexBuilder::new(&regex::escape(spoken_form))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        compiled.push(CompiledReplacementRule {
            regex,
            replacement: word.phrase.clone(),
            detail: format!("{} → {}", spoken_form, word.phrase),
            priority: 1,
            source_form: spoken_form.to_string(),
            source_kind: LibraryRuleSourceKind::CustomWordSpokenForm,
            language_scope: normalize_language_scope(word.language.as_deref()),
        });
    }

    let compiled = Arc::new(compiled);
    cache
        .lock()
        .expect("replacement rule cache poisoned")
        .insert(fingerprint, Arc::clone(&compiled));
    compiled
}

fn voice_command_rules_fingerprint(rules: &[VoiceCommandRule]) -> String {
    let mut fingerprint = String::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        fingerprint.push_str("V;i;");
        push_fingerprint_field(&mut fingerprint, rule.phrase.trim());
        push_fingerprint_field(&mut fingerprint, rule.output.trim());
        push_fingerprint_option(&mut fingerprint, rule.language.as_deref());
    }
    fingerprint
}

fn compiled_voice_command_rules(rules: &[VoiceCommandRule]) -> Arc<Vec<CompiledVoiceCommandRule>> {
    let fingerprint = voice_command_rules_fingerprint(rules);
    let cache = VOICE_COMMAND_RULE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(compiled) = cache
        .lock()
        .expect("voice command rule cache poisoned")
        .get(&fingerprint)
    {
        return Arc::clone(compiled);
    }

    let mut compiled = Vec::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let phrase = rule.phrase.trim();
        let Some(output) = voice_command_output(rule.output.trim()) else {
            continue;
        };
        if phrase.is_empty() {
            continue;
        }
        let Ok(regex) = RegexBuilder::new(&regex::escape(phrase))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        compiled.push(CompiledVoiceCommandRule {
            regex,
            phrase: phrase.to_string(),
            output,
            language_scope: normalize_language_scope(rule.language.as_deref()),
        });
    }

    let compiled = Arc::new(compiled);
    cache
        .lock()
        .expect("voice command rule cache poisoned")
        .insert(fingerprint, Arc::clone(&compiled));
    compiled
}

#[derive(Clone)]
struct ReplacementCandidate {
    start: usize,
    end: usize,
    replacement: String,
    detail: String,
    priority: u8,
    source_form: String,
    source_kind: LibraryRuleSourceKind,
    language_scope: Option<String>,
}

fn collect_replacement_candidates(
    text: &str,
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
) -> Vec<ReplacementCandidate> {
    let compiled = compiled_replacement_rules(replacements, custom_words);
    let mut candidates = Vec::new();

    for rule in compiled
        .iter()
        .filter(|rule| language_scope_matches(rule.language_scope.as_deref(), transcript_language))
    {
        for mat in rule.regex.find_iter(text) {
            if !candidate_has_boundaries(text, mat.start(), mat.end())
                || span_in_protected_token(text, mat.start(), mat.end())
            {
                continue;
            }
            candidates.push(ReplacementCandidate {
                start: mat.start(),
                end: mat.end(),
                replacement: rule.replacement.clone(),
                detail: rule.detail.clone(),
                priority: rule.priority,
                source_form: rule.source_form.clone(),
                source_kind: rule.source_kind,
                language_scope: rule.language_scope.clone(),
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

struct TextReplacementResult {
    text: String,
    operations: Vec<AppliedWritingOperation>,
    provenance: Vec<LibraryRuleApplication>,
}

fn apply_text_replacements_with_provenance(
    text: &str,
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
) -> TextReplacementResult {
    let candidates =
        collect_replacement_candidates(text, replacements, custom_words, transcript_language);
    if candidates.is_empty() {
        return TextReplacementResult {
            text: text.to_string(),
            operations: Vec::new(),
            provenance: Vec::new(),
        };
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
        return TextReplacementResult {
            text: text.to_string(),
            operations: Vec::new(),
            provenance: Vec::new(),
        };
    }

    let mut output = String::with_capacity(text.len());
    let mut operations = Vec::with_capacity(selected.len());
    let mut provenance = Vec::with_capacity(selected.len());
    let mut last = 0usize;

    for candidate in selected {
        output.push_str(&text[last..candidate.start]);
        let target_start = output.len();
        output.push_str(&candidate.replacement);
        let target_end = output.len();
        operations.push(AppliedWritingOperation {
            kind: WritingOperationKind::Replacement,
            detail: candidate.detail,
        });
        provenance.push(LibraryRuleApplication {
            source_form: candidate.source_form,
            target_text: candidate.replacement,
            source_kind: candidate.source_kind,
            language_scope: candidate.language_scope,
            start: candidate.start,
            end: candidate.end,
            target_start,
            target_end,
        });
        last = candidate.end;
    }
    output.push_str(&text[last..]);

    TextReplacementResult {
        text: output,
        operations,
        provenance,
    }
}

#[cfg(test)]
fn apply_text_replacements(
    text: &str,
    replacements: &[TextReplacementRule],
    custom_words: &[CustomWord],
    transcript_language: Option<&str>,
) -> (String, Vec<AppliedWritingOperation>) {
    let result = apply_text_replacements_with_provenance(
        text,
        replacements,
        custom_words,
        transcript_language,
    );
    (result.text, result.operations)
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

#[derive(Clone, Copy)]
pub(crate) enum VoiceCommandOutput {
    Punctuation(&'static str),
    Break(&'static str),
}

#[derive(Clone)]
struct VoiceCommandCandidate {
    start: usize,
    end: usize,
    phrase: String,
    output: VoiceCommandOutput,
}

pub(crate) fn voice_command_output(token: &str) -> Option<VoiceCommandOutput> {
    match token {
        "comma" => Some(VoiceCommandOutput::Punctuation(",")),
        "period" => Some(VoiceCommandOutput::Punctuation(".")),
        "question_mark" => Some(VoiceCommandOutput::Punctuation("?")),
        "exclamation_mark" => Some(VoiceCommandOutput::Punctuation("!")),
        "colon" => Some(VoiceCommandOutput::Punctuation(":")),
        "semicolon" => Some(VoiceCommandOutput::Punctuation(";")),
        "dash" => Some(VoiceCommandOutput::Punctuation("\u{2014}")),
        "new_line" => Some(VoiceCommandOutput::Break("\n")),
        "paragraph" => Some(VoiceCommandOutput::Break("\n\n")),
        _ => None,
    }
}

fn spans_overlap(left_start: usize, left_end: usize, right_start: usize, right_end: usize) -> bool {
    left_start < right_end && right_start < left_end
}

fn protected_span_contains(protected_spans: &[(usize, usize)], start: usize, end: usize) -> bool {
    protected_spans
        .iter()
        .any(|(protected_start, protected_end)| {
            spans_overlap(start, end, *protected_start, *protected_end)
        })
}

fn collect_voice_command_candidates(
    text: &str,
    rules: &[VoiceCommandRule],
    transcript_language: Option<&str>,
    protected_spans: &[(usize, usize)],
) -> Vec<VoiceCommandCandidate> {
    let compiled = compiled_voice_command_rules(rules);
    let mut candidates = Vec::new();
    for rule in compiled
        .iter()
        .filter(|rule| language_scope_matches(rule.language_scope.as_deref(), transcript_language))
    {
        for mat in rule.regex.find_iter(text) {
            if candidate_has_boundaries(text, mat.start(), mat.end())
                && !span_in_protected_token(text, mat.start(), mat.end())
                && !protected_span_contains(protected_spans, mat.start(), mat.end())
            {
                candidates.push(VoiceCommandCandidate {
                    start: mat.start(),
                    end: mat.end(),
                    phrase: rule.phrase.clone(),
                    output: rule.output,
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then((right.end - right.start).cmp(&(left.end - left.start)))
    });
    candidates
}

struct VoiceCommandResult {
    text: String,
    operations: Vec<AppliedWritingOperation>,
    index_map: Vec<Option<usize>>,
}

fn push_voice_output_char(
    output: &mut String,
    index_map: &mut [Option<usize>],
    original_index: Option<usize>,
    ch: char,
    suppress_space_after_break: &mut bool,
) {
    if *suppress_space_after_break && (ch == ' ' || ch == '\t') {
        return;
    }

    match ch {
        ',' | '.' | '?' | '!' | ':' | ';' | '\u{2014}' => {
            while output.ends_with(' ') || output.ends_with('\t') {
                output.pop();
            }
            if let Some(index) = original_index {
                index_map[index] = Some(output.len());
            }
            output.push(ch);
            if let Some(index) = original_index {
                index_map[index + ch.len_utf8()] = Some(output.len());
            }
            *suppress_space_after_break = false;
        }
        '\n' => {
            while output.ends_with(' ') || output.ends_with('\t') {
                output.pop();
            }
            if let Some(index) = original_index {
                index_map[index] = Some(output.len());
            }
            output.push('\n');
            if let Some(index) = original_index {
                index_map[index + ch.len_utf8()] = Some(output.len());
            }
            *suppress_space_after_break = true;
        }
        _ => {
            if let Some(index) = original_index {
                index_map[index] = Some(output.len());
            }
            output.push(ch);
            if let Some(index) = original_index {
                index_map[index + ch.len_utf8()] = Some(output.len());
            }
            *suppress_space_after_break = false;
        }
    }
}

fn push_voice_output(
    output: &mut String,
    index_map: &mut [Option<usize>],
    text: &str,
    original_start: Option<usize>,
    suppress_space_after_break: &mut bool,
) {
    for (relative_index, ch) in text.char_indices() {
        push_voice_output_char(
            output,
            index_map,
            original_start.map(|start| start + relative_index),
            ch,
            suppress_space_after_break,
        );
    }
}

fn apply_voice_commands_with_map(
    text: &str,
    settings: &WritingSettings,
    transcript_language: Option<&str>,
    protected_spans: &[(usize, usize)],
) -> VoiceCommandResult {
    let candidates = collect_voice_command_candidates(
        text,
        &settings.voice_commands,
        transcript_language,
        protected_spans,
    );
    if candidates.is_empty() {
        return VoiceCommandResult {
            text: text.to_string(),
            operations: Vec::new(),
            index_map: (0..=text.len()).map(Some).collect(),
        };
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
        return VoiceCommandResult {
            text: text.to_string(),
            operations: Vec::new(),
            index_map: (0..=text.len()).map(Some).collect(),
        };
    }

    let mut output = String::with_capacity(text.len());
    let mut operations = Vec::with_capacity(selected.len());
    let mut index_map = vec![None; text.len() + 1];
    index_map[0] = Some(0);
    let mut suppress_space_after_break = false;
    let mut last = 0usize;
    for candidate in selected {
        push_voice_output(
            &mut output,
            &mut index_map,
            &text[last..candidate.start],
            Some(last),
            &mut suppress_space_after_break,
        );
        let replacement = match candidate.output {
            VoiceCommandOutput::Punctuation(value) | VoiceCommandOutput::Break(value) => value,
        };
        push_voice_output(
            &mut output,
            &mut index_map,
            replacement,
            None,
            &mut suppress_space_after_break,
        );
        operations.push(AppliedWritingOperation {
            kind: WritingOperationKind::VoiceCommand,
            detail: format!("{} → {}", candidate.phrase, replacement.escape_debug()),
        });
        last = candidate.end;
    }
    push_voice_output(
        &mut output,
        &mut index_map,
        &text[last..],
        Some(last),
        &mut suppress_space_after_break,
    );
    index_map[text.len()] = Some(output.len());

    VoiceCommandResult {
        text: output,
        operations,
        index_map,
    }
}

#[cfg(test)]
pub(crate) fn apply_voice_commands(
    text: &str,
    settings: &WritingSettings,
    transcript_language: Option<&str>,
    protected_spans: &[(usize, usize)],
) -> (String, Vec<AppliedWritingOperation>) {
    let result =
        apply_voice_commands_with_map(text, settings, transcript_language, protected_spans);
    (result.text, result.operations)
}

fn remap_provenance_target_spans(
    index_map: &[Option<usize>],
    provenance: &mut [LibraryRuleApplication],
) {
    for application in provenance {
        if let (Some(start), Some(end)) = (
            index_map
                .get(application.target_start)
                .and_then(|value| *value),
            index_map
                .get(application.target_end)
                .and_then(|value| *value),
        ) {
            application.target_start = start;
            application.target_end = end;
        }
    }
}

pub(crate) fn apply_voice_command_stage(
    library_result: &mut LibraryRulesResult,
    settings: &WritingSettings,
    transcript_language: Option<&str>,
    applied_operations: &mut Vec<AppliedWritingOperation>,
) {
    if library_result.literal_locked {
        return;
    }

    let protected_spans: Vec<(usize, usize)> = library_result
        .provenance
        .iter()
        .map(|application| (application.target_start, application.target_end))
        .collect();
    let result = apply_voice_commands_with_map(
        &library_result.text,
        settings,
        transcript_language,
        &protected_spans,
    );
    library_result.text = result.text;
    remap_provenance_target_spans(&result.index_map, &mut library_result.provenance);
    applied_operations.extend(result.operations);
}

pub(crate) struct LibraryRulesResult {
    pub(crate) text: String,
    pub(crate) literal_locked: bool,
    pub(crate) provenance: Vec<LibraryRuleApplication>,
}

pub(crate) fn sanitize_transcript(text: &str) -> Cow<'_, str> {
    let trimmed = text.trim();
    let needs_cleanup = trimmed.len() != text.len()
        || trimmed
            .chars()
            .any(|ch| ch == '\r' || (ch.is_control() && ch != '\n' && ch != '\t'));

    if !needs_cleanup {
        return Cow::Borrowed(text);
    }

    let mut output = String::with_capacity(trimmed.len());
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            output.push('\n');
        } else if !ch.is_control() || ch == '\n' || ch == '\t' {
            output.push(ch);
        }
    }

    Cow::Owned(output)
}

pub(crate) fn apply_library_rules(
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
            provenance: Vec::new(),
        };
    }

    let replacement_result = apply_text_replacements_with_provenance(
        text,
        &settings.replacements,
        &settings.custom_words,
        transcript_language,
    );
    applied_operations.extend(replacement_result.operations);

    LibraryRulesResult {
        text: replacement_result.text,
        literal_locked: false,
        provenance: replacement_result.provenance,
    }
}

#[derive(Clone)]
struct FinalGuardCandidate {
    start: usize,
    end: usize,
    replacement: String,
    detail: String,
}

fn collect_final_guard_candidates(
    text: &str,
    provenance: &[LibraryRuleApplication],
) -> Vec<FinalGuardCandidate> {
    let mut candidates = Vec::new();
    for application in provenance {
        if application.source_form == application.target_text {
            continue;
        }
        let Ok(regex) = RegexBuilder::new(&regex::escape(&application.source_form))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };

        // Primary: re-apply at the exact recorded offset. This holds when AI
        // formatting left the term in place, or when AI never ran (text ==
        // library text). After a free-form AI rewrite, byte positions shift, so
        // the exact match silently fails and the reverted term is left
        // unrestored. Fallback: accept the nearest boundary-valid, non-protected
        // occurrence. Any such occurrence must be a reverted library term,
        // because the library already consumed every boundary-valid source form
        // into target_text — none survive into the text unless the AI
        // reintroduced one. The fallback is gated on `target_start` being a
        // plausible in-text position; in production offsets are always measured
        // in a related string, so this only rejects synthetic, out-of-range
        // provenance.
        let mut exact: Option<(usize, usize)> = None;
        let mut shifted: Option<(usize, usize)> = None;
        for mat in regex.find_iter(text) {
            if !candidate_has_boundaries(text, mat.start(), mat.end())
                || span_in_protected_token(text, mat.start(), mat.end())
            {
                continue;
            }
            if mat.start() == application.target_start {
                exact = Some((mat.start(), mat.end()));
            } else if application.target_start < text.len()
                && shifted.is_none_or(|(start, _)| {
                    mat.start().abs_diff(application.target_start)
                        < start.abs_diff(application.target_start)
                })
            {
                shifted = Some((mat.start(), mat.end()));
            }
        }

        if let Some((start, end)) = exact.or(shifted) {
            candidates.push(FinalGuardCandidate {
                start,
                end,
                replacement: application.target_text.clone(),
                detail: format!("{} → {}", application.source_form, application.target_text),
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then((right.end - right.start).cmp(&(left.end - left.start)))
    });
    candidates
}

pub(crate) fn apply_final_restoration_guard(
    text: &str,
    provenance: &[LibraryRuleApplication],
    literal_locked: bool,
    needs_output_language_transform: bool,
) -> (String, Vec<AppliedWritingOperation>) {
    if literal_locked || needs_output_language_transform || provenance.is_empty() {
        return (text.to_string(), Vec::new());
    }

    let candidates = collect_final_guard_candidates(text, provenance);
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
            kind: WritingOperationKind::FinalGuard,
            detail: candidate.detail,
        });
        last = candidate.end;
    }
    output.push_str(&text[last..]);

    (output, operations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_text_replacements_prefers_explicit_rules() {
        let replacements = vec![TextReplacementRule {
            from: "voice typer".to_string(),
            to: "Voicetypr".to_string(),
            language: Some("en".to_string()),
            enabled: true,
        }];
        let custom_words = vec![CustomWord {
            phrase: "Voicetypr".to_string(),
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

        assert_eq!(text, "Voicetypr rules");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, WritingOperationKind::Replacement);
    }

    #[test]
    fn test_custom_word_spoken_form_creates_correction() {
        let (text, ops) = apply_text_replacements(
            "voice typer launched",
            &[],
            &[CustomWord {
                phrase: "Voicetypr".to_string(),
                spoken_form: Some("voice typer".to_string()),
                language: Some("en".to_string()),
                enabled: true,
            }],
            Some("en"),
        );

        assert_eq!(text, "Voicetypr launched");
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
    fn test_replacements_protect_word_tokens_urls_and_emails() {
        let replacements = vec![TextReplacementRule {
            from: "foo".to_string(),
            to: "bar".to_string(),
            language: Some("en".to_string()),
            enabled: true,
        }];

        let (protected_text, protected_ops) = apply_text_replacements(
            "foo_bar https://example.com/foo user@foo.com",
            &replacements,
            &[],
            Some("en"),
        );
        assert_eq!(
            protected_text,
            "foo_bar https://example.com/foo user@foo.com"
        );
        assert!(protected_ops.is_empty());

        let (boundary_text, boundary_ops) =
            apply_text_replacements("foo foo, (foo) end foo", &replacements, &[], Some("en"));
        assert_eq!(boundary_text, "bar bar, (bar) end bar");
        assert_eq!(boundary_ops.len(), 4);
    }

    #[test]
    fn test_replacement_provenance_records_selected_rules() {
        let result = apply_text_replacements_with_provenance(
            "voice typer uses react",
            &[TextReplacementRule {
                from: "voice typer".to_string(),
                to: "Voicetypr".to_string(),
                language: Some("en".to_string()),
                enabled: true,
            }],
            &[CustomWord {
                phrase: "React".to_string(),
                spoken_form: Some("react".to_string()),
                language: Some("en".to_string()),
                enabled: true,
            }],
            Some("en"),
        );

        assert_eq!(result.text, "Voicetypr uses React");
        assert_eq!(result.provenance.len(), 2);
        assert_eq!(result.provenance[0].source_form, "voice typer");
        assert_eq!(result.provenance[0].target_text, "Voicetypr");
        assert_eq!(
            result.provenance[0].source_kind,
            LibraryRuleSourceKind::ExplicitReplacement
        );
        assert_eq!(
            (result.provenance[0].start, result.provenance[0].end),
            (0, 11)
        );
        assert_eq!(
            (
                result.provenance[0].target_start,
                result.provenance[0].target_end
            ),
            (0, 9)
        );
        assert_eq!(result.provenance[1].source_form, "react");
        assert_eq!(result.provenance[1].target_text, "React");
        assert_eq!(
            result.provenance[1].source_kind,
            LibraryRuleSourceKind::CustomWordSpokenForm
        );
    }

    #[test]
    fn test_replacement_provenance_tracks_only_applied_rules() {
        let result = apply_text_replacements_with_provenance(
            "voice typer",
            &[TextReplacementRule {
                from: "voice typer".to_string(),
                to: "Voicetypr".to_string(),
                language: Some("en".to_string()),
                enabled: true,
            }],
            &[
                CustomWord {
                    phrase: "Voicetypr".to_string(),
                    spoken_form: Some("voice typer".to_string()),
                    language: Some("en".to_string()),
                    enabled: true,
                },
                CustomWord {
                    phrase: "Sean".to_string(),
                    spoken_form: None,
                    language: Some("en".to_string()),
                    enabled: true,
                },
                CustomWord {
                    phrase: "Bonjour".to_string(),
                    spoken_form: Some("hello".to_string()),
                    language: Some("fr".to_string()),
                    enabled: true,
                },
            ],
            Some("en"),
        );

        assert_eq!(result.text, "Voicetypr");
        assert_eq!(result.provenance.len(), 1);
        assert_eq!(
            result.provenance[0].source_kind,
            LibraryRuleSourceKind::ExplicitReplacement
        );
    }

    #[test]
    fn test_final_guard_restores_only_provenance_sources() {
        let replacement_result = apply_text_replacements_with_provenance(
            "voice typer is fast",
            &[TextReplacementRule {
                from: "voice typer".to_string(),
                to: "Voicetypr".to_string(),
                language: Some("en".to_string()),
                enabled: true,
            }],
            &[],
            Some("en"),
        );

        let (guarded, ops) = apply_final_restoration_guard(
            "voice typer is fast",
            &replacement_result.provenance,
            false,
            false,
        );

        assert_eq!(guarded, "Voicetypr is fast");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, WritingOperationKind::FinalGuard);

        let (unproven, unproven_ops) =
            apply_final_restoration_guard("voice typer is fast", &[], false, false);
        assert_eq!(unproven, "voice typer is fast");
        assert!(unproven_ops.is_empty());
    }

    #[test]
    fn test_final_guard_skips_literal_and_language_transform_paths() {
        let provenance = vec![LibraryRuleApplication {
            source_form: "voice typer".to_string(),
            target_text: "Voicetypr".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 11,
            target_start: 0,
            target_end: 9,
        }];

        let (literal_text, literal_ops) =
            apply_final_restoration_guard("voice typer", &provenance, true, false);
        assert_eq!(literal_text, "voice typer");
        assert!(literal_ops.is_empty());

        let (translated_text, translated_ops) =
            apply_final_restoration_guard("voice typer", &provenance, false, true);
        assert_eq!(translated_text, "voice typer");
        assert!(translated_ops.is_empty());
    }

    #[test]
    fn test_final_guard_respects_boundaries() {
        let provenance = vec![LibraryRuleApplication {
            source_form: "react".to_string(),
            target_text: "React".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 5,
            target_start: 20,
            target_end: 25,
        }];

        let (guarded, ops) =
            apply_final_restoration_guard("createReactiveStore react", &provenance, false, false);

        assert_eq!(guarded, "createReactiveStore React");
        assert_eq!(ops.len(), 1);

        let (shifted, shifted_ops) =
            apply_final_restoration_guard("please react", &provenance, false, false);
        assert_eq!(shifted, "please react");
        assert!(shifted_ops.is_empty());
    }

    #[test]
    fn test_final_guard_restores_after_position_shift() {
        // Simulates AI formatting that prepended "well, " and reverted the
        // library casing, shifting the term off its recorded offset
        // (target_start == 0). The guard must still restore it rather than
        // silently no-op'ing.
        let provenance = vec![LibraryRuleApplication {
            source_form: "voice typer".to_string(),
            target_text: "Voicetypr".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 11,
            target_start: 0,
            target_end: 9,
        }];

        let (guarded, ops) =
            apply_final_restoration_guard("well, voice typer", &provenance, false, false);
        assert_eq!(guarded, "well, Voicetypr");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, WritingOperationKind::FinalGuard);

        // An out-of-range recorded offset (beyond the text) is left untouched:
        // the fallback only fires for plausible in-text positions, so synthetic
        // or inconsistent provenance never over-substitutes.
        let out_of_range = vec![LibraryRuleApplication {
            source_form: "voice typer".to_string(),
            target_text: "Voicetypr".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 11,
            target_start: 50,
            target_end: 59,
        }];
        let (untouched, untouched_ops) =
            apply_final_restoration_guard("well, voice typer", &out_of_range, false, false);
        assert_eq!(untouched, "well, voice typer");
        assert!(untouched_ops.is_empty());
    }

    #[test]
    fn test_final_guard_skips_protected_tokens() {
        let url_provenance = vec![LibraryRuleApplication {
            source_form: "foo".to_string(),
            target_text: "bar".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 3,
            target_start: 20,
            target_end: 23,
        }];
        let (url_text, url_ops) =
            apply_final_restoration_guard("https://example.com/foo", &url_provenance, false, false);
        assert_eq!(url_text, "https://example.com/foo");
        assert!(url_ops.is_empty());

        let email_provenance = vec![LibraryRuleApplication {
            source_form: "foo".to_string(),
            target_text: "bar".to_string(),
            source_kind: LibraryRuleSourceKind::ExplicitReplacement,
            language_scope: Some("en".to_string()),
            start: 0,
            end: 3,
            target_start: 5,
            target_end: 8,
        }];
        let (email_text, email_ops) =
            apply_final_restoration_guard("user@foo.com", &email_provenance, false, false);
        assert_eq!(email_text, "user@foo.com");
        assert!(email_ops.is_empty());
    }

    #[test]
    fn test_sanitize_transcript_is_mechanical_only() {
        let cleaned = sanitize_transcript(" \r\nhello\rworld\tthere\0\u{0008} ");
        assert_eq!(cleaned.as_ref(), "hello\nworld\tthere");

        let semantic_text = "um I mean send it to Bob no Alice period";
        let untouched = sanitize_transcript(semantic_text);
        assert!(matches!(untouched, Cow::Borrowed(_)));
        assert_eq!(untouched.as_ref(), semantic_text);
    }

    #[test]
    fn test_library_rules_run_after_mechanical_cleanup() {
        let settings = WritingSettings {
            snippets: vec![Snippet {
                trigger: "insert note".to_string(),
                body: "Saved body".to_string(),
                language: Some("en".to_string()),
                enabled: true,
                preserve_literal: true,
            }],
            ..WritingSettings::default()
        };
        let cleaned = sanitize_transcript("\r\ninsert note\n");
        let mut ops = Vec::new();
        let result = apply_library_rules(cleaned.as_ref(), &settings, Some("en"), &mut ops);

        assert_eq!(result.text, "Saved body");
        assert!(result.literal_locked);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, WritingOperationKind::Snippet);
    }

    #[test]
    fn test_voice_commands_apply_punctuation_and_breaks() {
        let settings = WritingSettings::default();
        let (text, ops) = apply_voice_commands(
            "hello insert comma world insert period first line new line second line new paragraph done",
            &settings,
            Some("en"),
            &[],
        );

        assert_eq!(text, "hello, world. first line\nsecond line\n\ndone");
        assert_eq!(ops.len(), 4);
        assert!(ops
            .iter()
            .all(|op| op.kind == WritingOperationKind::VoiceCommand));
    }

    #[test]
    fn test_default_voice_commands_are_english_scoped() {
        let settings = WritingSettings::default();

        let (english_text, english_ops) =
            apply_voice_commands("hello insert period", &settings, Some("en"), &[]);
        assert_eq!(english_text, "hello.");
        assert_eq!(english_ops.len(), 1);

        let (french_text, french_ops) =
            apply_voice_commands("bonjour insert period", &settings, Some("fr"), &[]);
        assert_eq!(french_text, "bonjour insert period");
        assert!(french_ops.is_empty());
    }

    #[test]
    fn test_voice_commands_respect_boundaries_and_language() {
        let settings = WritingSettings::default();
        let (boundary_text, boundary_ops) = apply_voice_commands(
            "the Jurassic period used comma separated values",
            &settings,
            Some("en"),
            &[],
        );
        assert_eq!(
            boundary_text,
            "the Jurassic period used comma separated values"
        );
        assert!(boundary_ops.is_empty());

        let scoped_settings = WritingSettings {
            voice_commands: vec![VoiceCommandRule {
                phrase: "virgule".to_string(),
                output: "comma".to_string(),
                language: Some("fr".to_string()),
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let (language_text, language_ops) =
            apply_voice_commands("hello virgule world", &scoped_settings, Some("en"), &[]);
        assert_eq!(language_text, "hello virgule world");
        assert!(language_ops.is_empty());
    }

    #[test]
    fn test_voice_commands_skip_protected_tokens() {
        let settings = WritingSettings {
            voice_commands: vec![VoiceCommandRule {
                phrase: "foo".to_string(),
                output: "comma".to_string(),
                language: Some("en".to_string()),
                enabled: true,
            }],
            ..WritingSettings::default()
        };

        let (text, ops) = apply_voice_commands(
            "foo_bar https://example.com/foo user@foo.com foo",
            &settings,
            Some("en"),
            &[],
        );

        assert_eq!(text, "foo_bar https://example.com/foo user@foo.com,");
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn test_voice_commands_apply_user_rules_and_skip_disabled_or_mismatched() {
        let settings = WritingSettings {
            voice_commands: vec![
                VoiceCommandRule {
                    phrase: "slash".to_string(),
                    output: "dash".to_string(),
                    language: None,
                    enabled: true,
                },
                VoiceCommandRule {
                    phrase: "quiet".to_string(),
                    output: "comma".to_string(),
                    language: None,
                    enabled: false,
                },
                VoiceCommandRule {
                    phrase: "virgule".to_string(),
                    output: "comma".to_string(),
                    language: Some("fr".to_string()),
                    enabled: true,
                },
            ],
            ..WritingSettings::default()
        };

        let (text, ops) = apply_voice_commands(
            "one slash two quiet three virgule four",
            &settings,
            Some("en"),
            &[],
        );

        assert_eq!(text, "one— two quiet three virgule four");
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn test_voice_command_stage_skips_literal_snippets() {
        let mut result = LibraryRulesResult {
            text: "hello comma world".to_string(),
            literal_locked: true,
            provenance: Vec::new(),
        };
        let mut ops = Vec::new();
        let settings = WritingSettings::default();

        apply_voice_command_stage(&mut result, &settings, Some("en"), &mut ops);

        assert_eq!(result.text, "hello comma world");
        assert!(ops.is_empty());
    }

    #[test]
    fn test_voice_command_stage_protects_library_outputs() {
        let mut result = LibraryRulesResult {
            text: "New Line Cinema made Period Tracker".to_string(),
            literal_locked: false,
            provenance: vec![
                LibraryRuleApplication {
                    source_form: "new line cinema".to_string(),
                    target_text: "New Line Cinema".to_string(),
                    source_kind: LibraryRuleSourceKind::ExplicitReplacement,
                    language_scope: Some("en".to_string()),
                    start: 0,
                    end: 15,
                    target_start: 0,
                    target_end: 15,
                },
                LibraryRuleApplication {
                    source_form: "period tracker".to_string(),
                    target_text: "Period Tracker".to_string(),
                    source_kind: LibraryRuleSourceKind::CustomWordSpokenForm,
                    language_scope: Some("en".to_string()),
                    start: 21,
                    end: 35,
                    target_start: 21,
                    target_end: 35,
                },
            ],
        };
        let mut ops = Vec::new();
        let settings = WritingSettings::default();

        apply_voice_command_stage(&mut result, &settings, Some("en"), &mut ops);

        assert_eq!(result.text, "New Line Cinema made Period Tracker");
        assert!(ops.is_empty());
    }

    #[test]
    fn test_voice_commands_refresh_final_guard_positions() {
        let settings = WritingSettings {
            custom_words: vec![CustomWord {
                phrase: "Voicetypr".to_string(),
                spoken_form: Some("voice typer".to_string()),
                language: Some("en".to_string()),
                enabled: true,
            }],
            ..WritingSettings::default()
        };
        let mut ops = Vec::new();
        let mut result = apply_library_rules(
            "hello insert comma voice typer",
            &settings,
            Some("en"),
            &mut ops,
        );

        apply_voice_command_stage(&mut result, &settings, Some("en"), &mut ops);

        assert_eq!(result.text, "hello, Voicetypr");
        assert_eq!(result.provenance[0].target_start, 7);
        let (guarded, guard_ops) =
            apply_final_restoration_guard("hello, voice typer", &result.provenance, false, false);
        assert_eq!(guarded, "hello, Voicetypr");
        assert_eq!(guard_ops.len(), 1);
    }

    #[test]
    fn test_voice_commands_do_not_resolve_semantic_cleanup() {
        let settings = WritingSettings::default();
        let (text, ops) = apply_voice_commands(
            "um I mean send it to Bob no Alice",
            &settings,
            Some("en"),
            &[],
        );

        assert_eq!(text, "um I mean send it to Bob no Alice");
        assert!(ops.is_empty());
    }
}
