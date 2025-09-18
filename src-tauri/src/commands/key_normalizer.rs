use keyboard_types::{Code, Key};
use std::str::FromStr;

/// Normalize keyboard shortcut keys for cross-platform compatibility
/// Converts frontend key names to Tauri-compatible format
pub fn normalize_shortcut_keys(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|key_str| normalize_single_key(key_str))
        .collect::<Vec<_>>()
        .join("+")
}

/// Normalize a single key string
fn normalize_single_key(key: &str) -> &str {
    // First check case-insensitive matches for common modifiers
    match key.to_lowercase().as_str() {
        "cmd" => return "CommandOrControl",
        "ctrl" => return "CommandOrControl",
        "control" => return "Control",  // Keep Control separate for macOS Cmd+Ctrl support
        "command" => return "CommandOrControl",
        "meta" => return "CommandOrControl",
        "super" => return "Super",  // Super is Command on macOS
        "option" => return "Alt",
        "alt" => return "Alt",
        "shift" => return "Shift",
        "space" => return "Space",
        _ => {}
    }

    // First, try to parse as keyboard_types::Key for semantic normalization
    if let Ok(parsed_key) = Key::from_str(key) {
        match parsed_key {
            Key::Enter => "Enter",
            Key::Tab => "Tab",
            Key::Backspace => "Backspace",
            Key::Escape => "Escape",
            Key::Character(s) if s == " " => "Space",
            Key::ArrowDown => "Down",
            Key::ArrowLeft => "Left",
            Key::ArrowRight => "Right",
            Key::ArrowUp => "Up",
            Key::End => "End",
            Key::Home => "Home",
            Key::PageDown => "PageDown",
            Key::PageUp => "PageUp",
            Key::Delete => "Delete",
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            _ => key, // Return original if no normalization needed
        }
    } else {
        // Handle special cases that might not parse
        match key {
            "Return" => "Enter",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "CommandOrControl" => "CommandOrControl", // Keep as-is for Tauri
            "Cmd" => "CommandOrControl",
            "Ctrl" => "CommandOrControl",
            "Control" => "Control",  // Keep Control separate
            "Command" => "CommandOrControl",
            "Super" => "Super",  // Super is Command on macOS
            "Option" => "Alt",
            "Meta" => "CommandOrControl",
            _ => key,
        }
    }
}

/// Validation rules for key combinations
#[derive(Debug, Clone)]
pub struct KeyValidationRules {
    pub min_keys: usize,
    pub max_keys: usize,
    pub require_modifier: bool,
    pub require_modifier_for_multi_key: bool,
}

impl KeyValidationRules {
    /// Standard hotkey rules (2-5 keys, must include at least one modifier)
    pub fn standard() -> Self {
        Self {
            min_keys: 2,
            max_keys: 5,
            require_modifier: true,
            require_modifier_for_multi_key: true,
        }
    }
}

/// Validate that a key combination is allowed with default rules
pub fn validate_key_combination(shortcut: &str) -> Result<(), String> {
    validate_key_combination_with_rules(shortcut, &KeyValidationRules::standard())
}

/// Validate that a key combination is allowed with custom rules
pub fn validate_key_combination_with_rules(
    shortcut: &str,
    rules: &KeyValidationRules,
) -> Result<(), String> {
    let parts: Vec<&str> = shortcut.split('+').collect();

    // Check minimum keys
    if parts.len() < rules.min_keys {
        return Err(format!("Minimum {} key(s) required", rules.min_keys));
    }

    // Check maximum keys
    if parts.len() > rules.max_keys {
        return Err(format!(
            "Maximum {} keys allowed in combination",
            rules.max_keys
        ));
    }

    // Check modifier requirements
    let is_modifier = |key: &str| -> bool {
        matches!(
            key,
            "CommandOrControl"
                | "Super"
                | "Shift"
                | "Alt"
                | "Control"
                | "Command"
                | "Cmd"
                | "Ctrl"
                | "Option"
                | "Meta"
        )
    };

    let has_modifier = parts.iter().any(|&key| is_modifier(key));

    if rules.require_modifier && !has_modifier {
        return Err("At least one modifier key is required".to_string());
    }

    // Check that the shortcut starts with a modifier
    if rules.require_modifier && !parts.is_empty() && !is_modifier(parts[0]) {
        return Err(
            "Keyboard shortcuts must start with a modifier key (Cmd/Ctrl, Alt, or Shift)"
                .to_string(),
        );
    }

    if rules.require_modifier_for_multi_key && !has_modifier && parts.len() > 1 {
        return Err("Multi-key shortcuts must include at least one modifier key".to_string());
    }

    // Validate each key
    for key in parts {
        if !is_valid_key(key) {
            return Err(format!("Invalid key: {}", key));
        }
    }

    Ok(())
}

/// Check if a key string is valid
fn is_valid_key(key: &str) -> bool {
    // Empty keys are invalid
    if key.is_empty() {
        return false;
    }

    // Try to parse as keyboard_types Key or Code first
    if Key::from_str(key).is_ok() || Code::from_str(key).is_ok() {
        return true;
    }

    // Allow any non-empty string as a potential key
    // The actual validation will happen when we try to register the shortcut
    // This allows for maximum flexibility with different keyboard layouts and OS-specific keys
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_shortcut_keys() {
        // Test default shortcut remains unchanged
        assert_eq!(
            normalize_shortcut_keys("CommandOrControl+Shift+Space"),
            "CommandOrControl+Shift+Space"
        );

        assert_eq!(normalize_shortcut_keys("Return"), "Enter");
        assert_eq!(normalize_shortcut_keys("ArrowUp"), "Up");
        assert_eq!(
            normalize_shortcut_keys("CommandOrControl+ArrowDown"),
            "CommandOrControl+Down"
        );
        assert_eq!(normalize_shortcut_keys("Shift+Return"), "Shift+Enter");
        assert_eq!(
            normalize_shortcut_keys("Cmd+Shift+Space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(normalize_shortcut_keys("Space"), "Space");
        assert_eq!(normalize_shortcut_keys("F1"), "F1");

        // Test case-insensitive normalization
        assert_eq!(
            normalize_shortcut_keys("cmd+shift+space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(
            normalize_shortcut_keys("ctrl+shift+space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(
            normalize_shortcut_keys("CMD+SHIFT+SPACE"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(normalize_shortcut_keys("alt+a"), "Alt+a");
        assert_eq!(normalize_shortcut_keys("SHIFT+F1"), "Shift+F1");

        // Test modifier keys stay as-is (no special single key handling anymore)
        assert_eq!(normalize_shortcut_keys("Alt"), "Alt");
        assert_eq!(normalize_shortcut_keys("Shift"), "Shift");
        assert_eq!(normalize_shortcut_keys("Control"), "Control");
        assert_eq!(normalize_shortcut_keys("Ctrl"), "CommandOrControl");
        assert_eq!(normalize_shortcut_keys("Command"), "CommandOrControl");
        assert_eq!(normalize_shortcut_keys("Cmd"), "CommandOrControl");

        // In combinations, they get normalized too
        assert_eq!(normalize_shortcut_keys("Alt+A"), "Alt+A");
        assert_eq!(normalize_shortcut_keys("Shift+Space"), "Shift+Space");

        // Test Super modifier (Command on macOS)
        assert_eq!(normalize_shortcut_keys("Super+A"), "Super+A");
        assert_eq!(normalize_shortcut_keys("Super+Control+A"), "Super+Control+A");
        assert_eq!(normalize_shortcut_keys("Super+Control+Alt+A"), "Super+Control+Alt+A");
        assert_eq!(normalize_shortcut_keys("Super+Control+Alt+Shift+A"), "Super+Control+Alt+Shift+A");
    }

    #[test]
    fn test_validate_key_combination() {
        assert!(validate_key_combination("CommandOrControl+A").is_ok());
        assert!(validate_key_combination("Shift+Alt+F1").is_ok());
        assert!(validate_key_combination("A").is_err()); // Single key not allowed
        assert!(validate_key_combination("A+B").is_err()); // Multi-key without modifier not allowed
        assert!(validate_key_combination("A+CommandOrControl").is_err()); // Must start with modifier
        assert!(validate_key_combination("Space+Shift").is_err()); // Must start with modifier
        assert!(validate_key_combination("Cmd+Shift+Alt+Ctrl+A+B").is_err()); // Too many keys
        assert!(validate_key_combination("CommandOrControl+InvalidKey").is_ok()); // Any non-empty key is valid

        // Test punctuation keys
        assert!(validate_key_combination("CommandOrControl+/").is_ok());
        assert!(validate_key_combination("Cmd+\\").is_ok());
        assert!(validate_key_combination("Ctrl+,").is_ok());
        assert!(validate_key_combination("Shift+.").is_ok());
        assert!(validate_key_combination("Alt+;").is_ok());
        assert!(validate_key_combination("CommandOrControl+[").is_ok());
        assert!(validate_key_combination("CommandOrControl+]").is_ok());
        assert!(validate_key_combination("Cmd+-").is_ok());
        assert!(validate_key_combination("Cmd+=").is_ok());
        assert!(validate_key_combination("/").is_err()); // Single punctuation key not allowed

        // Test special named keys
        assert!(validate_key_combination("CommandOrControl+Slash").is_ok());
        assert!(validate_key_combination("Cmd+BracketLeft").is_ok());
        assert!(validate_key_combination("Ctrl+Minus").is_ok());

        // Test numpad keys
        assert!(validate_key_combination("CommandOrControl+Numpad0").is_ok());
        assert!(validate_key_combination("Alt+NumpadAdd").is_ok());
        assert!(validate_key_combination("Shift+NumpadEnter").is_ok());

        // Test media keys
        assert!(validate_key_combination("MediaPlayPause").is_err()); // Single media key not allowed
        assert!(validate_key_combination("AudioVolumeUp").is_err()); // Single media key not allowed

        // Test system keys
        assert!(validate_key_combination("PrintScreen").is_err()); // Single system key not allowed
        assert!(validate_key_combination("Cmd+Insert").is_ok());
        assert!(validate_key_combination("Alt+CapsLock").is_ok());

        // Test number row symbols
        assert!(validate_key_combination("Shift+1").is_ok()); // !
        assert!(validate_key_combination("Shift+2").is_ok()); // @
        assert!(validate_key_combination("Cmd+Shift+3").is_ok()); // #

        // Test international keys
        assert!(validate_key_combination("CommandOrControl+ü").is_ok());
        assert!(validate_key_combination("Alt+ñ").is_ok());

        // Test Super modifier combinations (Command on macOS)
        assert!(validate_key_combination("Super+A").is_ok());
        assert!(validate_key_combination("Super+Control+A").is_ok());
        assert!(validate_key_combination("Super+Control+Alt+A").is_ok());
        assert!(validate_key_combination("Super+Control+Alt+Shift+A").is_ok());
        assert!(validate_key_combination("Control+Alt+A").is_ok());
        assert!(validate_key_combination("Control+Shift+A").is_ok());
    }

    #[test]
    fn test_single_modifier_parsing() {
        // Test that demonstrates the difference between single modifiers and combinations
        use tauri_plugin_global_shortcut::Shortcut;

        // These should fail to parse as single keys
        assert!(
            "Alt".parse::<Shortcut>().is_err(),
            "Alt alone should not parse"
        );
        assert!(
            "Shift".parse::<Shortcut>().is_err(),
            "Shift alone should not parse"
        );
        assert!(
            "Control".parse::<Shortcut>().is_err(),
            "Control alone should not parse"
        );

        // Let's test what actually works for single keys
        let test_keys = vec![
            "LeftAlt",
            "RightAlt",
            "LeftShift",
            "RightShift",
            "LeftControl",
            "RightControl",
            "LeftMeta",
            "RightMeta",
            "A",
            "B",
            "Space",
            "F1",
            "Tab",
            "CapsLock",
        ];

        for key in test_keys {
            match key.parse::<Shortcut>() {
                Ok(_) => println!("{} parses successfully", key),
                Err(e) => println!("{} failed to parse: {:?}", key, e),
            }
        }

        // Combinations with generic modifiers should work
        assert!("Alt+A".parse::<Shortcut>().is_ok(), "Alt+A should parse");
        assert!(
            "Shift+Space".parse::<Shortcut>().is_ok(),
            "Shift+Space should parse"
        );
    }
}
