use crate::commands::audio::{
    is_duplicate_transcription, page_history_keys, reconcile_transcription_history_entry,
    TranscriptionStatus,
};
use serde_json::json;

const CURRENT_SESSION_MARKER: &str = "current-session-marker";
const PRIOR_SESSION_MARKER: &str = "prior-session-marker";
const STALE_FAILURE_TEXT: &str = "Retranscription interrupted before completion";

fn retranscription_row(
    status: Option<TranscriptionStatus>,
    text: &str,
    session_marker: Option<&str>,
) -> serde_json::Value {
    let mut row = json!({
        "text": text,
        "model": "base",
        "timestamp": "2026-03-31T00:00:00Z",
        "recording_file": "recordings/retry.wav",
        "source_recording_id": "recordings/original.wav",
        "is_retranscription": true,
    });

    if let Some(status) = status {
        row["status"] = json!(status);
    }

    if let Some(session_marker) = session_marker {
        row["retranscription_session_marker"] = json!(session_marker);
    }

    row
}

#[test]
fn stale_prior_session_in_progress_retranscription_becomes_failed() {
    let input = retranscription_row(
        Some(TranscriptionStatus::InProgress),
        "In progress...",
        Some(PRIOR_SESSION_MARKER),
    );

    let output = reconcile_transcription_history_entry(input.clone(), CURRENT_SESSION_MARKER);

    assert_eq!(output["status"], json!(TranscriptionStatus::Failed));
    assert_eq!(output["text"], json!(STALE_FAILURE_TEXT));
    assert_eq!(output["recording_file"], input["recording_file"]);
    assert_eq!(output["source_recording_id"], input["source_recording_id"]);
    assert_eq!(output["is_retranscription"], json!(true));
    assert!(output.get("retranscription_session_marker").is_none());
    assert_eq!(
        output["failure_detail"],
        json!({
            "kind": "stale_retranscription_session",
            "current_session_marker": CURRENT_SESSION_MARKER,
            "stale_session_marker": PRIOR_SESSION_MARKER,
        })
    );
}

#[test]
fn current_session_in_progress_retranscription_remains_active() {
    let input = retranscription_row(
        Some(TranscriptionStatus::InProgress),
        "In progress...",
        Some(CURRENT_SESSION_MARKER),
    );

    let output = reconcile_transcription_history_entry(input.clone(), CURRENT_SESSION_MARKER);

    assert_eq!(output, input);
}

#[test]
fn missing_status_normalizes_to_completed() {
    let input = retranscription_row(None, "Finished text", Some(PRIOR_SESSION_MARKER));

    let output = reconcile_transcription_history_entry(input.clone(), CURRENT_SESSION_MARKER);

    assert_eq!(output["status"], json!(TranscriptionStatus::Completed));
    assert_eq!(output["text"], input["text"]);
    assert_eq!(output["recording_file"], input["recording_file"]);
    assert_eq!(output["source_recording_id"], input["source_recording_id"]);
    assert!(output.get("retranscription_session_marker").is_none());
    assert!(output.get("failure_detail").is_none());
}

#[test]
fn reconciliation_is_idempotent() {
    let input = retranscription_row(
        Some(TranscriptionStatus::InProgress),
        "In progress...",
        Some(PRIOR_SESSION_MARKER),
    );

    let first_pass = reconcile_transcription_history_entry(input, CURRENT_SESSION_MARKER);
    let second_pass =
        reconcile_transcription_history_entry(first_pass.clone(), CURRENT_SESSION_MARKER);

    assert_eq!(first_pass, second_pass);
}

#[test]
fn completed_and_failed_rows_remain_unchanged() {
    let completed = retranscription_row(
        Some(TranscriptionStatus::Completed),
        "Ready text",
        Some(PRIOR_SESSION_MARKER),
    );
    let failed = json!({
        "text": "Remote transcription failed - re-transcribe after resolving the issue",
        "model": "base",
        "timestamp": "2026-03-31T00:00:00Z",
        "recording_file": "recordings/retry.wav",
        "source_recording_id": "recordings/original.wav",
        "is_retranscription": true,
        "status": TranscriptionStatus::Failed,
        "failure_detail": {
            "kind": "remote_http_status",
            "message": "Server error: 502 Bad Gateway"
        }
    });

    assert_eq!(
        reconcile_transcription_history_entry(completed.clone(), CURRENT_SESSION_MARKER),
        completed
    );
    assert_eq!(
        reconcile_transcription_history_entry(failed.clone(), CURRENT_SESSION_MARKER),
        failed
    );
}

#[test]
fn page_returns_newest_first_limited() {
    let keys = vec![
        "2026-03-31T00:00:00Z".to_string(),
        "2026-04-01T00:00:00Z".to_string(),
        "2026-04-02T00:00:00Z".to_string(),
        "2026-04-03T00:00:00Z".to_string(),
        "2026-04-04T00:00:00Z".to_string(),
    ];

    assert_eq!(
        page_history_keys(keys, 2),
        vec![
            "2026-04-04T00:00:00Z".to_string(),
            "2026-04-03T00:00:00Z".to_string()
        ]
    );
}

#[test]
fn page_orders_mixed_rfc3339_offsets_by_timestamp() {
    let keys = vec![
        "2026-06-10T12:00:00Z".to_string(),
        "2026-06-10T12:00:01+00:00".to_string(),
        "2026-06-10T11:59:59-00:00".to_string(),
    ];

    assert_eq!(
        page_history_keys(keys, 3),
        vec![
            "2026-06-10T12:00:01+00:00".to_string(),
            "2026-06-10T12:00:00Z".to_string(),
            "2026-06-10T11:59:59-00:00".to_string(),
        ]
    );
}

#[test]
fn reconciliation_applies_to_returned_page() {
    let key = "2026-04-04T00:00:00Z".to_string();
    let page = page_history_keys(vec![key], 1);
    let row = retranscription_row(
        Some(TranscriptionStatus::InProgress),
        "In progress...",
        Some(PRIOR_SESSION_MARKER),
    );

    let returned = page
        .into_iter()
        .map(|_| reconcile_transcription_history_entry(row.clone(), CURRENT_SESSION_MARKER))
        .collect::<Vec<_>>();

    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0]["status"], json!(TranscriptionStatus::Failed));
    assert_eq!(returned[0]["text"], json!(STALE_FAILURE_TEXT));
}

#[test]
fn stale_rows_outside_page_untouched() {
    let newest_key = "2026-04-04T00:00:00Z".to_string();
    let stale_key = "2026-04-03T00:00:00Z".to_string();
    let stale_row = retranscription_row(
        Some(TranscriptionStatus::InProgress),
        "In progress...",
        Some(PRIOR_SESSION_MARKER),
    );
    let mut rows = [
        (
            newest_key.clone(),
            retranscription_row(
                Some(TranscriptionStatus::Completed),
                "Ready text",
                Some(PRIOR_SESSION_MARKER),
            ),
        ),
        (stale_key.clone(), stale_row.clone()),
    ];
    let page = page_history_keys(vec![stale_key.clone(), newest_key], 1);

    for page_key in page {
        if let Some((_, row)) = rows.iter_mut().find(|(key, _)| key == &page_key) {
            *row = reconcile_transcription_history_entry(row.clone(), CURRENT_SESSION_MARKER);
        }
    }

    let outside_page = rows
        .iter()
        .find(|(key, _)| key == &stale_key)
        .map(|(_, row)| row)
        .unwrap();
    assert_eq!(outside_page, &stale_row);
}

#[test]
fn duplicate_save_guard_uses_latest_entry() {
    let latest_key = "2026-06-10T12:00:00Z";
    let latest = json!({
        "text": "same text",
        "model": "base",
        "timestamp": latest_key,
    });
    let now = chrono::DateTime::parse_from_rfc3339(latest_key)
        .unwrap()
        .with_timezone(&chrono::Utc);

    assert!(is_duplicate_transcription(
        latest_key,
        &latest,
        "same text",
        "base",
        now
    ));
    assert!(!is_duplicate_transcription(
        latest_key,
        &latest,
        "same text",
        "base",
        now + chrono::Duration::seconds(3)
    ));
}
