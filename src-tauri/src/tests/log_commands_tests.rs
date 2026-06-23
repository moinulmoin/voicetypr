#[cfg(test)]
mod tests {
    use crate::commands::logs::{
        find_newest_log, read_log_tail, redact_log_content, LatestLogAttachment,
    };
    use std::io::Write;
    use tempfile::TempDir;

    // ── find_newest_log ────────────────────────────────────────────────

    #[test]
    fn test_find_newest_log_selects_most_recent() {
        let dir = TempDir::new().unwrap();
        let path_a = dir.path().join("voicetypr-2026-04-25.log");
        let path_b = dir.path().join("voicetypr-2026-04-26.log");
        let path_c = dir.path().join("voicetypr-2026-04-27.log");

        std::fs::write(&path_a, "old").unwrap();
        std::fs::write(&path_b, "mid").unwrap();
        std::fs::write(&path_c, "new").unwrap();

        let newest = find_newest_log(dir.path());
        assert!(newest.is_some());
        assert_eq!(newest.unwrap(), path_c);
    }

    #[test]
    fn test_find_newest_log_returns_none_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert!(find_newest_log(dir.path()).is_none());
    }

    #[test]
    fn test_find_newest_log_returns_none_for_nonexistent_dir() {
        let nonexistent = std::path::Path::new("/tmp/voicetypr-nonexistent-test-dir-zzz");
        assert!(find_newest_log(nonexistent).is_none());
    }

    #[test]
    fn test_find_newest_log_returns_none_when_no_matching_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("other.log"), "data").unwrap();
        std::fs::write(dir.path().join("voicetypr-readme.txt"), "data").unwrap();
        assert!(find_newest_log(dir.path()).is_none());
    }

    #[test]
    fn test_find_newest_log_ignores_extensionless_names() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("voicetypr-2026-04-27"), "data").unwrap();
        // extensionless — should not match
        assert!(find_newest_log(dir.path()).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn test_find_newest_log_ignores_symlinked_logs() {
        let dir = TempDir::new().unwrap();
        let outside = dir.path().join("outside.txt");
        let link = dir.path().join("voicetypr-2026-04-27.log");
        std::fs::write(&outside, "not really a log").unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        assert!(find_newest_log(dir.path()).is_none());
    }

    // ── read_log_tail ──────────────────────────────────────────────────

    #[test]
    fn test_read_log_tail_full_file_when_small() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.log");
        let content = "line 1\nline 2\nline 3\n";
        std::fs::write(&path, content).unwrap();

        let (result, original_len, truncated) = read_log_tail(&path, 1024).unwrap();
        assert!(!truncated);
        assert_eq!(original_len, content.len() as u64);
        assert_eq!(result, content);
    }

    #[test]
    fn test_read_log_tail_truncates_large_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("large.log");

        // Build a file of ~100 lines, request tail of ~5 lines
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..100 {
            writeln!(file, "line number {}", i).unwrap();
        }
        let full_size = std::fs::metadata(&path).unwrap().len();

        // Request a small tail
        let (result, original_len, truncated) = read_log_tail(&path, 200).unwrap();
        assert!(
            result.len() <= 200,
            "tail read exceeded requested byte bound"
        );
        assert!(truncated);
        assert_eq!(original_len, full_size);
        // Should only contain the last few lines
        let line_count = result.lines().count();
        assert!(line_count < 100, "got {} lines, expected fewer", line_count);
        assert!(line_count > 1, "should have at least a couple lines");
        assert!(result.contains("line number 99"));
        // Should not contain line number 0
        assert!(!result.contains("line number 0"));
    }

    #[test]
    fn test_read_log_tail_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.log");
        std::fs::write(&path, "").unwrap();

        let (result, original_len, truncated) = read_log_tail(&path, 1024).unwrap();
        assert!(!truncated);
        assert_eq!(original_len, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_log_tail_uses_lossy_utf8_for_malformed_logs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("malformed.log");
        let bytes = b"line before\n\xFF\xFEline after";
        std::fs::write(&path, bytes).unwrap();

        let (result, original_len, truncated) = read_log_tail(&path, 1024).unwrap();
        assert!(!truncated);
        assert_eq!(original_len, bytes.len() as u64);
        assert!(result.contains("line before"));
        assert!(result.contains("line after"));
    }

    #[test]
    fn test_read_log_tail_keeps_long_line_without_newline() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("long.log");
        let content = format!("{}tail", "a".repeat(500));
        std::fs::write(&path, content).unwrap();

        let (result, _original_len, truncated) = read_log_tail(&path, 40).unwrap();
        assert!(truncated);
        assert!(!result.is_empty());
        assert!(result.ends_with("tail"));
    }

    // ── redact_log_content ─────────────────────────────────────────────

    #[test]
    fn test_redact_sk_api_keys() {
        let input = "Using key sk-abc123def456ghijklmnopqrstuvwxyz for API";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("abc123def456ghijklmnopqrstuvwxyz"));
        assert!(redacted.contains("sk-[REDACTED]"));
        assert!(redacted.contains("for API"));
    }

    #[test]
    fn test_redact_anthropic_keys() {
        let input = "Authorization: sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz123456"));
        assert!(redacted.contains("sk-ant-[REDACTED]"));
    }

    #[test]
    fn test_redact_bearer_tokens() {
        let input = "header: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ";
        let redacted = redact_log_content(input);
        assert!(redacted.contains("bearer [REDACTED]"));
        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_redact_api_key_assignments() {
        let input = "api_key = \"sk-secret-value-here\" and done";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("sk-secret-value-here"));
        assert!(redacted.contains("api_key"));
        assert!(redacted.contains("and done"));
    }

    #[test]
    fn test_redact_quoted_license_key_fields() {
        let input = r#"Raw cached data: {"license_key":"ABCD-1234-EFGH-5678-IJKL", "api_key": String("sk-secret-value-here")}"#;
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("ABCD-1234-EFGH-5678-IJKL"));
        assert!(!redacted.contains("sk-secret-value-here"));
        assert!(redacted.contains("license_key"));
        assert!(redacted.contains("api_key"));
    }

    #[test]
    fn test_redact_email_addresses() {
        let input = "User: alice@example.com and bob@voicetypr.com reported";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("alice@example.com"));
        assert!(!redacted.contains("bob@voicetypr.com"));
        assert!(redacted.contains("[EMAIL_REDACTED]"));
        // Count: both emails redacted but [EMAIL_REDACTED] appears for each
        let email_redactions = redacted.matches("[EMAIL_REDACTED]").count();
        assert_eq!(email_redactions, 2);
    }

    #[test]
    fn test_redact_license_like_patterns() {
        let input = "Activating license ABCD-1234-EFGH-5678-IJKL for pro tier";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("ABCD-1234-EFGH-5678-IJKL"));
        assert!(redacted.contains("[LICENSE_REDACTED]"));
        assert!(redacted.contains("for pro tier"));
    }

    #[test]
    fn test_redact_home_directory_paths() {
        let input = "Config loaded from /Users/janedoe/.config/voicetypr/settings.json";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("/Users/janedoe"));
        assert!(redacted.contains("[HOME_DIR]"));
        assert!(redacted.contains("/.config/voicetypr/settings.json"));
    }

    #[test]
    fn test_redact_non_ascii_home_directory_paths() {
        let input = "Config loaded from /home/josé/.config/voicetypr/settings.json";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("/home/josé"));
        assert!(redacted.contains("[HOME_DIR]"));
        assert!(redacted.contains("/.config/voicetypr/settings.json"));
    }

    #[test]
    fn test_redact_home_directory_paths_with_spaces() {
        let input = "Config loaded from /Users/jane doe/Library/Application Support/Voicetypr/settings.json";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("/Users/jane doe"));
        assert!(redacted.contains("[HOME_DIR]"));
    }

    #[test]
    fn test_redact_license_pattern_keeps_unlabeled_build_ids() {
        let input = "Build 2022-04-15-RELEASE completed before license ABCD-1234-EFGH-5678-IJKL";
        let redacted = redact_log_content(input);
        assert!(redacted.contains("2022-04-15-RELEASE"));
        assert!(!redacted.contains("ABCD-1234-EFGH-5678-IJKL"));
        assert!(redacted.contains("license [LICENSE_REDACTED]"));
    }

    #[test]
    fn test_redact_external_absolute_paths() {
        let input = "Saved image to /Volumes/private-drive/customer/audio/output.png and C:\\Temp\\voice.txt";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("/Volumes/private-drive/customer/audio/output.png"));
        assert!(!redacted.contains("C:\\Temp\\voice.txt"));
        assert!(redacted.contains("[PATH_REDACTED]"));
    }

    #[test]
    fn test_redact_macos_library_paths() {
        let input = "Log file at /Library/Application Support/Voicetypr/system.log";
        let redacted = redact_log_content(input);
        assert!(!redacted.contains("/Library/Application"));
        assert!(!redacted.contains("Support/Voicetypr/system.log"));
        assert!(redacted.contains("[PATH_REDACTED]"));
    }

    #[test]
    fn test_redact_preserves_non_sensitive_content() {
        let input = "INFO: Transcription completed in 2.3s for model whisper-base";
        let redacted = redact_log_content(input);
        assert_eq!(redacted, input);
    }

    #[test]
    fn test_redact_empty_string() {
        assert_eq!(redact_log_content(""), "");
    }

    // ── LatestLogAttachment serialization ──────────────────────────────

    #[test]
    fn test_latest_log_attachment_serialization() {
        let attachment = LatestLogAttachment {
            file_name: Some("voicetypr-2026-04-27.log".to_string()),
            redacted_content: "[REDACTED] log content".to_string(),
            truncated: true,
            status_note: String::new(),
        };

        let json = serde_json::to_string(&attachment).unwrap();
        assert!(json.contains("voicetypr-2026-04-27.log"));
        assert!(json.contains("[REDACTED] log content"));
        assert!(json.contains("\"fileName\":\"voicetypr-2026-04-27.log\""));
        assert!(json.contains("\"redactedContent\":\"[REDACTED] log content\""));
        assert!(json.contains("\"truncated\":true"));
    }

    #[test]
    fn test_latest_log_attachment_no_log_state() {
        let attachment = LatestLogAttachment {
            file_name: None,
            redacted_content: String::new(),
            truncated: false,
            status_note: "No log file found.".to_string(),
        };

        let json = serde_json::to_string(&attachment).unwrap();
        assert!(json.contains("\"fileName\":null"));
        assert!(json.contains("No log file found."));
        assert!(json.contains("\"truncated\":false"));
    }
}
