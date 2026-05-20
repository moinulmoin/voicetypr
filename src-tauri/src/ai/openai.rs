pub(crate) fn model_uses_max_completion_tokens(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized.starts_with("gpt-5")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
}

pub(crate) fn is_unsupported_token_parameter_error(error_text: &str, parameter_name: &str) -> bool {
    let haystack = error_text.to_ascii_lowercase();
    let parameter = parameter_name.to_ascii_lowercase();
    let unsupported_single = format!("unsupported parameter: '{}'", parameter);
    let unsupported_double = format!("unsupported parameter: \"{}\"", parameter);
    let unsupported_bare = format!("unsupported parameter: {}", parameter);
    let unsupported_code = format!("unsupported_parameter: {}", parameter);
    let unsupported_code_quoted = format!("unsupported_parameter: \"{}\"", parameter);
    let type_field_compact = "\"type\":\"unsupported_parameter\"";
    let type_field_spaced = "\"type\": \"unsupported_parameter\"";
    let param_field_compact = format!("\"param\":\"{}\"", parameter);
    let param_field_spaced = format!("\"param\": \"{}\"", parameter);

    haystack.contains(&unsupported_single)
        || haystack.contains(&unsupported_double)
        || haystack.contains(&unsupported_bare)
        || haystack.contains(&unsupported_code)
        || haystack.contains(&unsupported_code_quoted)
        || ((haystack.contains(type_field_compact) || haystack.contains(type_field_spaced))
            && (haystack.contains(&param_field_compact) || haystack.contains(&param_field_spaced)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_uses_max_completion_tokens() {
        assert!(model_uses_max_completion_tokens("gpt-5"));
        assert!(model_uses_max_completion_tokens("gpt-5-mini"));
        assert!(model_uses_max_completion_tokens("o1"));
        assert!(model_uses_max_completion_tokens("o1-mini"));
        assert!(model_uses_max_completion_tokens("o3-mini"));
        assert!(model_uses_max_completion_tokens("o4-mini"));

        assert!(!model_uses_max_completion_tokens("gpt-4o"));
        assert!(!model_uses_max_completion_tokens("gpt-4.1"));
    }

    #[test]
    fn test_unsupported_token_parameter_error_detection() {
        let error = "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.";
        assert!(is_unsupported_token_parameter_error(error, "max_tokens"));
        assert!(!is_unsupported_token_parameter_error(
            error,
            "max_completion_tokens"
        ));

        let error = "unsupported_parameter: max_completion_tokens";
        assert!(is_unsupported_token_parameter_error(
            error,
            "max_completion_tokens"
        ));

        let error = r#"{"type":"unsupported_parameter","param":"max_tokens"}"#;
        assert!(is_unsupported_token_parameter_error(error, "max_tokens"));
        assert!(!is_unsupported_token_parameter_error(
            error,
            "max_completion_tokens"
        ));
    }
}
