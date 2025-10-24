#[cfg(test)]
mod tray_onboarding_tests {
    #[test]
    fn test_tray_label_when_onboarding_not_completed_returns_none() {
        let label = crate::format_tray_model_label(
            false,
            "parakeet-tdt-0.6b-v3",
            Some("Parakeet Tiny V3".to_string()),
        );
        assert_eq!(label, "Model: None");
    }

    #[test]
    fn test_should_mark_model_selected_respects_onboarding() {
        assert_eq!(
            crate::should_mark_model_selected(false, "x", "x"),
            false
        );
        assert_eq!(crate::should_mark_model_selected(true, "x", "x"), true);
        assert_eq!(crate::should_mark_model_selected(true, "x", "y"), false);
    }

    #[test]
    fn test_tray_label_when_onboarding_complete_uses_display_name() {
        let label = crate::format_tray_model_label(
            true,
            "parakeet-tdt-0.6b-v3",
            Some("Parakeet Tiny V3".to_string()),
        );
        assert_eq!(label, "Model: Parakeet Tiny V3");
    }
}
