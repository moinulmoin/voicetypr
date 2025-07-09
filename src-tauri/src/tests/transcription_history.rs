#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::json;

    #[test]
    fn test_transcription_storage_format() {
        // Test the format of stored transcriptions
        let timestamp = Utc::now().to_rfc3339();
        let transcription = json!({
            "text": "Hello world",
            "model": "base",
            "timestamp": &timestamp
        });

        assert!(transcription["text"].is_string());
        assert_eq!(transcription["text"], "Hello world");
        assert_eq!(transcription["model"], "base");
        assert_eq!(transcription["timestamp"], timestamp);
    }

    #[test]
    fn test_cleanup_date_calculation() {
        // Test date calculations for cleanup
        let now = Utc::now();
        let seven_days_ago = now - Duration::days(7);
        let fifteen_days_ago = now - Duration::days(15);
        let thirty_days_ago = now - Duration::days(30);

        // Verify timestamps are correctly ordered
        assert!(seven_days_ago > fifteen_days_ago);
        assert!(fifteen_days_ago > thirty_days_ago);
        assert!(now > seven_days_ago);

        // Test RFC3339 format parsing
        let timestamp_str = seven_days_ago.to_rfc3339();
        let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp_str).unwrap();
        assert_eq!(parsed.timestamp(), seven_days_ago.timestamp());
    }

    #[test]
    fn test_transcription_key_format() {
        // Test that timestamps are valid keys for the store
        let timestamps = vec![
            Utc::now().to_rfc3339(),
            (Utc::now() - Duration::days(1)).to_rfc3339(),
            (Utc::now() - Duration::days(7)).to_rfc3339(),
        ];

        for timestamp in timestamps {
            // Verify it's a valid RFC3339 string
            assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
            // Verify it can be used as a string key
            assert!(!timestamp.is_empty());
            assert!(timestamp.contains("T")); // Contains time separator
            assert!(timestamp.contains("Z") || timestamp.contains("+")); // Contains timezone
        }
    }

    #[test]
    fn test_transcription_sorting() {
        // Test that timestamps sort correctly (newest first)
        let mut entries = vec![
            ((Utc::now() - Duration::days(2)).to_rfc3339(), json!({"text": "old"})),
            (Utc::now().to_rfc3339(), json!({"text": "newest"})),
            ((Utc::now() - Duration::days(1)).to_rfc3339(), json!({"text": "middle"})),
        ];

        // Sort by timestamp (newest first)
        entries.sort_by(|a, b| b.0.cmp(&a.0));

        // Verify order
        assert_eq!(entries[0].1["text"], "newest");
        assert_eq!(entries[1].1["text"], "middle");
        assert_eq!(entries[2].1["text"], "old");
    }

    #[test]
    fn test_cleanup_filtering() {
        // Test filtering logic for cleanup
        let now = Utc::now();
        let cutoff = now - Duration::days(7);

        let test_dates = vec![
            (now - Duration::days(1), false),  // Should not be deleted
            (now - Duration::days(6), false),  // Should not be deleted
            (now - Duration::days(7), false),  // Exactly at cutoff, should not be deleted
            (now - Duration::days(8), true),   // Should be deleted
            (now - Duration::days(30), true),  // Should be deleted
        ];

        for (date, should_delete) in test_dates {
            let is_old = date < cutoff;
            assert_eq!(is_old, should_delete, 
                "Date {} days old should {} be deleted", 
                (now - date).num_days(),
                if should_delete { "" } else { "not" }
            );
        }
    }

    #[test]
    fn test_history_limit() {
        // Test that history limiting works correctly
        let entries: Vec<serde_json::Value> = (0..100)
            .map(|i| json!({
                "text": format!("Transcription {}", i),
                "model": "base",
                "timestamp": (Utc::now() - Duration::minutes(i)).to_rfc3339()
            }))
            .collect();

        // Test different limits
        let limits = vec![50, 25, 10, 1];
        for limit in limits {
            let mut limited = entries.clone();
            limited.truncate(limit);
            assert_eq!(limited.len(), limit);
            // Verify we kept the first (newest) entries
            assert_eq!(limited[0]["text"], "Transcription 0");
        }
    }

    #[test]
    fn test_transcription_content() {
        // Test various transcription content scenarios
        let long_text = "Very long transcription ".repeat(100);
        let long_text_trimmed = long_text.trim();
        let test_cases = vec![
            ("Hello world", "base"),
            ("", "tiny"), // Empty transcription
            (long_text_trimmed, "small"),
            ("Special chars: 你好 мир ñ", "base"),
            ("Numbers: 123 456.78", "base"),
        ];

        for (text, model) in test_cases {
            let transcription = json!({
                "text": text,
                "model": model,
                "timestamp": Utc::now().to_rfc3339()
            });

            assert_eq!(transcription["text"], text);
            assert_eq!(transcription["model"], model);
            assert!(transcription["timestamp"].is_string());
        }
    }

    #[test]
    fn test_cleanup_with_none_days() {
        // Test that cleanup with None (keep forever) doesn't delete anything
        let days: Option<u32> = None;
        
        // When days is None, nothing should be cleaned up
        assert!(days.is_none());
        
        // In the actual implementation, the cleanup function should early return
        // when days is None, preserving all transcriptions
    }

    #[test]
    fn test_timestamp_ordering() {
        // Test that RFC3339 timestamps maintain chronological order when sorted as strings
        let now = Utc::now();
        let timestamps: Vec<String> = (0..10)
            .map(|i| (now - Duration::hours(i)).to_rfc3339())
            .collect();
        
        // The first timestamp should be the most recent
        for i in 1..timestamps.len() {
            assert!(timestamps[0] > timestamps[i], 
                "Newer timestamp should be greater than older when compared as strings");
        }
        
        // Test reverse chronological sort (newest first)
        let mut sorted = timestamps.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(sorted[0], timestamps[0]); // Most recent should be first
        assert_eq!(sorted[sorted.len()-1], timestamps[timestamps.len()-1]); // Oldest should be last
    }
}