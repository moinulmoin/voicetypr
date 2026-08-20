//! Soniox cloud STT: async Files + Transcriptions + poll flow.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "stt-async-v5";

const BASE: &str = "https://api.soniox.com/v1";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.soniox.com/v1/models",
        AuthScheme::Bearer,
        key,
        "Soniox",
    )
    .await
    .map_err(|e| e.message("Soniox"))
}

/// API base for the async Files + Transcriptions flow. Tests override this
/// via [`set_base_url_override`] to point the whole flow at a wiremock server.
fn base_url() -> String {
    BASE_OVERRIDE.with(|cell| {
        cell.borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| BASE.to_string())
    })
}

thread_local! {
    static BASE_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_base_url_override(base: Option<String>) {
    BASE_OVERRIDE.with(|cell| *cell.borrow_mut() = base);
}

/// Best-effort cleanup of Soniox stored records (plan 044): deleting a
/// transcription cascades to its file; if creation never succeeded the
/// uploaded file is orphaned and is deleted directly. A 409 means the job is
/// still processing (poll-timeout path) — the record lingers until Soniox's
/// ~30-day purge or the settings cleanup action drains it; never fatal.
async fn cleanup_stored_records(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<&str>,
    file_id: &str,
) {
    let (url, what) = match transcription_id {
        Some(tid) => (
            format!("{}/transcriptions/{tid}", base_url()),
            "transcription",
        ),
        None => (format!("{}/files/{file_id}", base_url()), "file"),
    };
    let attempt = client.delete(&url).bearer_auth(key).send();
    match tokio::time::timeout(std::time::Duration::from_secs(10), attempt).await {
        Ok(Ok(resp)) if resp.status().is_success() => {}
        Ok(Ok(resp)) => {
            log::warn!(
                "Soniox cleanup: delete {what} returned HTTP {} ({url})",
                resp.status()
            )
        }
        Ok(Err(e)) => log::warn!("Soniox cleanup: delete {what} request failed: {e}"),
        Err(_) => log::warn!("Soniox cleanup: delete {what} timed out ({url})"),
    }
}

/// Writing-settings context for transcription hints; failures degrade to no
/// context rather than failing the dictation.
fn load_soniox_context(
    app: &AppHandle,
    language: Option<&str>,
) -> Option<crate::writing::SonioxContext> {
    match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_soniox_context(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Soniox context; continuing without context: {err}"
            );
            None
        }
    }
}

// ---- Stored-record management (plan 044) ----
//
// Soniox retains every uploaded file AND every transcription record against
// org caps (default 1,000 files / 2,000 transcriptions). Dictations now
// delete their own records on completion; these commands cover the backlog
// (and any 409-stuck records) from before that behavior existed.

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SonioxStorageCounts {
    pub files_total: u64,
    pub transcriptions_total: u64,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SonioxCleanupResult {
    pub deleted_transcriptions: u64,
    pub deleted_files: u64,
    pub skipped_processing: u64,
    pub errors: Vec<String>,
}

fn stored_key(app: &AppHandle) -> Result<String, String> {
    crate::secure_store::secure_get(app, crate::cloud_stt::CloudProvider::Soniox.key_name())?
        .ok_or_else(|| "Soniox API key not set".to_string())
}

async fn get_total(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
) -> Result<u64, common::SttError> {
    let url = format!("{}/{collection}/count", base_url());
    let resp = client
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| common::classify_reqwest_err(&e))?;
    if !resp.status().is_success() {
        return Err(common::log_http_body(resp, "Soniox storage count").await);
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    Ok(json.get("total").and_then(|v| v.as_u64()).unwrap_or(0))
}

/// Cursor-paginated id listing for a collection ("files" | "transcriptions").
/// Defensively accepts `items`/`data` array keys; Soniox documents
/// `next_page_cursor` and a 1000 item page cap.
async fn list_record_ids(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
) -> Result<Vec<String>, common::SttError> {
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!("{}/{collection}?limit=1000", base_url());
        if let Some(c) = &cursor {
            url.push_str(&format!("&cursor={c}"));
        }
        let resp = client
            .get(&url)
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| common::classify_reqwest_err(&e))?;
        if !resp.status().is_success() {
            return Err(common::log_http_body(resp, "Soniox storage list").await);
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;
        let items = json
            .get(collection)
            .and_then(|v| v.as_array())
            .or_else(|| json.get("items").and_then(|v| v.as_array()))
            .or_else(|| json.get("data").and_then(|v| v.as_array()));
        if let Some(items) = items {
            for item in items {
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    ids.push(id.to_string());
                }
            }
        }
        cursor = json
            .get("next_page_cursor")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|c| !c.is_empty());
        if cursor.is_none() || ids.len() > 100_000 {
            break;
        }
    }
    Ok(ids)
}

/// One paced delete: 409 = record still processing (skipped), 429 = file-
/// management RPM (back off once, then count as an error). File-management
/// RPM is real but its numeric limit is undocumented — pace every request.
enum DeleteOutcome {
    Deleted,
    /// 409: record still processing — leave it for a later pass.
    SkippedProcessing,
    /// 404: already gone (e.g. cascaded by a transcription delete, or raced
    /// with another cleanup) — the goal state, not an error.
    AlreadyGone,
}

async fn delete_one(
    client: &reqwest::Client,
    key: &str,
    url: &str,
) -> Result<DeleteOutcome, String> {
    for attempt in 0..2 {
        let resp = client
            .delete(url)
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.is_success() {
            return Ok(DeleteOutcome::Deleted);
        }
        if status == reqwest::StatusCode::CONFLICT {
            return Ok(DeleteOutcome::SkippedProcessing);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(DeleteOutcome::AlreadyGone);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        return Err(format!("HTTP {status}"));
    }
    Err("rate limited".to_string())
}

pub(crate) async fn storage_counts(app: &AppHandle) -> Result<SonioxStorageCounts, String> {
    let key = stored_key(app)?;
    let client = common::http_client();
    let (files, transcriptions) = tokio::try_join!(
        get_total(&client, &key, "files"),
        get_total(&client, &key, "transcriptions")
    )
    .map_err(|e| e.message("Soniox"))?;
    Ok(SonioxStorageCounts {
        files_total: files,
        transcriptions_total: transcriptions,
    })
}

pub(crate) async fn cleanup_stored(app: &AppHandle) -> Result<SonioxCleanupResult, String> {
    use tauri::Emitter;

    let key = stored_key(app)?;
    let client = common::http_client();

    let report = |done: u64, total: u64| {
        let _ = app.emit(
            "soniox-cleanup-progress",
            serde_json::json!({ "deleted": done, "total": total }),
        );
    };

    drain_stored_records(&client, &key, Some(&report)).await
}

/// Deletes every stored transcription record (each cascading to its file),
/// then lists files AGAIN and deletes only the true orphans — listing before
/// the transcription pass would issue guaranteed-404 deletes for every
/// cascaded file, wasting the rate-limited file-management API and reporting
/// phantom errors. Pacing: Soniox's file-management RPM is real but its
/// numeric limit is undocumented. Shared by the settings clean-up command
/// (progress events) and the background auto-clean (counter only).
async fn drain_stored_records(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<SonioxCleanupResult, String> {
    let mut result = SonioxCleanupResult::default();

    let transcription_ids = list_record_ids(client, key, "transcriptions")
        .await
        .map_err(|e| e.message("Soniox"))?;
    // Placeholder until the post-transcription listing replaces it.
    let mut total_ops = transcription_ids.len() as u64;
    let mut done_ops: u64 = 0;

    for id in &transcription_ids {
        let url = format!("{}/transcriptions/{id}", base_url());
        match delete_one(client, key, &url).await {
            Ok(DeleteOutcome::Deleted) => {
                result.deleted_transcriptions += 1;
                bump_auto_cleanup_progress();
            }
            Ok(DeleteOutcome::SkippedProcessing) => result.skipped_processing += 1,
            Ok(DeleteOutcome::AlreadyGone) => {}
            Err(e) => result.errors.push(format!("transcription {id}: {e}")),
        }
        done_ops += 1;
        if done_ops % 25 == 0 {
            if let Some(report) = report {
                report(done_ops, total_ops);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Fresh listing AFTER the transcription deletes: cascaded files are
    // already gone server-side, so only genuine orphans remain to delete.
    let file_ids = list_record_ids(client, key, "files")
        .await
        .map_err(|e| e.message("Soniox"))?;
    total_ops += file_ids.len() as u64;
    for id in &file_ids {
        let url = format!("{}/files/{id}", base_url());
        match delete_one(client, key, &url).await {
            Ok(DeleteOutcome::Deleted) => {
                result.deleted_files += 1;
                bump_auto_cleanup_progress();
            }
            Ok(DeleteOutcome::AlreadyGone) => {}
            // Files have no processing state; a 409 here is unexpected.
            Ok(DeleteOutcome::SkippedProcessing) => {
                result.errors.push(format!("file {id}: HTTP 409"))
            }
            Err(e) => result.errors.push(format!("file {id}: {e}")),
        }
        done_ops += 1;
        if done_ops % 25 == 0 {
            if let Some(report) = report {
                report(done_ops, total_ops);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    if let Some(report) = report {
        report(done_ops, total_ops);
    }
    Ok(result)
}

// --- Storage-limit self-heal (plan 044) --------------------------------------
//
// When a dictation hits Soniox's storage wall, drain the stored records in
// the background and retry once — for most users the cap is only reachable
// via pre-044 backlog, so the first limit hit self-heals and the dictation
// succeeds a few seconds later with no visible error. If the retry still
// hits the wall, the flow returns LimitExceeded and the caller surfaces the
// settings toast.

static AUTO_CLEANUP_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static AUTO_CLEANUP_DELETED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump_auto_cleanup_progress() {
    AUTO_CLEANUP_DELETED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Starts the background drain unless one is already running. Each deletion
/// immediately frees org capacity, so a blocked dictation only needs the
/// FIRST deletion before retrying.
fn spawn_auto_cleanup(client: reqwest::Client, key: String) {
    use std::sync::atomic::Ordering;
    if AUTO_CLEANUP_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        log::info!("Soniox storage limit hit: background cleanup started");
        match drain_stored_records(&client, &key, None).await {
            Ok(totals) => log::info!(
                "Soniox background cleanup finished: {} transcriptions + {} files deleted, {} skipped, {} errors",
                totals.deleted_transcriptions,
                totals.deleted_files,
                totals.skipped_processing,
                totals.errors.len()
            ),
            Err(e) => log::warn!("Soniox background cleanup failed: {e}"),
        }
        AUTO_CLEANUP_RUNNING.store(false, Ordering::SeqCst);
    });
}

/// Waits (polled, bounded) until the auto-cleanup has freed at least one
/// record since the wait started, or `budget` elapses. The retry attempt is
/// the source of truth either way.
async fn wait_for_cleanup_progress(budget: std::time::Duration) {
    use std::sync::atomic::Ordering;
    let start = std::time::Instant::now();
    let baseline = AUTO_CLEANUP_DELETED.load(Ordering::SeqCst);
    while start.elapsed() < budget {
        if AUTO_CLEANUP_DELETED.load(Ordering::SeqCst) > baseline {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Frontend escalation when the storage-limit self-heal did NOT succeed:
/// same shape as the license-required flow — bring the dashboard to front
/// and let the main window navigate itself to the Soniox stored-files card.
/// No toast action needed: the user lands directly on the fix.
async fn notify_storage_limit(app: &AppHandle) {
    use tauri::Emitter;
    let _ = crate::commands::window::focus_main_window(app.clone()).await;
    let _ = app.emit(
        "soniox-storage-limit",
        serde_json::json!({
            "title": "Soniox storage limit reached",
            "message": "Automatic cleanup could not free enough space. Delete stored files here, then dictate again.",
            "autoHealed": false,
        }),
    );
}

fn build_create_payload(
    file_id: &str,
    language: Option<&str>,
    context: Option<crate::writing::SonioxContext>,
    diarize: bool,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": MODEL,
        "file_id": file_id,
    });

    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        payload["language_hints"] = serde_json::json!([lang]);
    }

    if let Some(context) = context {
        if let Ok(context_value) = serde_json::to_value(context) {
            if context_value
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                payload["context"] = context_value;
            }
        }
    }

    if diarize {
        payload["enable_speaker_diarization"] = serde_json::json!(true);
    }

    payload
}

pub(super) async fn transcribe_typed(
    app: &AppHandle,
    key: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let client = common::http_client();
    let soniox_context = load_soniox_context(app, language);

    let result =
        transcribe_typed_with_autoheal(&client, key, wav_path, wav_bytes, language, soniox_context)
            .await;
    if matches!(result, Err(common::SttError::LimitExceeded)) {
        notify_storage_limit(app).await;
    }
    result
}

/// Upload → transcribe → delete-records with the plan-044 storage-limit
/// self-heal: on `LimitExceeded`, a background cleanup drains stored records
/// (each deletion immediately frees capacity); once the first record is gone
/// the WHOLE flow restarts from upload — the just-uploaded file may itself
/// have been deleted by the cleanup, so the create step must not be reused.
/// One retry; a second limit wall is terminal.
async fn transcribe_typed_with_autoheal(
    client: &reqwest::Client,
    key: &str,
    wav_path: &Path,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<String, common::SttError> {
    const LIMIT_ATTEMPTS: usize = 2;
    for attempt in 1..=LIMIT_ATTEMPTS {
        match attempt_typed_once(
            client,
            key,
            wav_path,
            &wav_bytes,
            language,
            soniox_context.clone(),
        )
        .await
        {
            Ok(text) => return Ok(text),
            Err(e) if matches!(e, common::SttError::LimitExceeded) && attempt < LIMIT_ATTEMPTS => {
                spawn_auto_cleanup(client.clone(), key.to_string());
                wait_for_cleanup_progress(std::time::Duration::from_secs(8)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("attempt loop always returns within LIMIT_ATTEMPTS")
}

/// One full attempt: upload + create/poll/extract + record cleanup.
async fn attempt_typed_once(
    client: &reqwest::Client,
    key: &str,
    wav_path: &Path,
    wav_bytes: &[u8],
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<String, common::SttError> {
    use reqwest::multipart::{Form, Part};

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", base_url());
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.to_vec();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);

            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox upload").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    let (transcription_id, result) =
        run_typed_transcription(client, key, &file_id, language, soniox_context).await;

    // Soniox stores every uploaded file + transcription record against the
    // org's caps (1k files / 2k transcriptions). Delete-after-extract on ALL
    // exits — success included (plan 044).
    cleanup_stored_records(client, key, transcription_id.as_deref(), &file_id).await;
    result
}

/// Steps 2-4 of the typed flow: create transcription, poll to terminal
/// status, extract text. App-free so tests can drive it against wiremock.
/// Returns the transcription id (once created) alongside the outcome so the
/// caller can always clean up the stored records.
async fn run_typed_transcription(
    client: &reqwest::Client,
    key: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (Option<String>, Result<String, common::SttError>) {
    // 2) Create transcription -> transcription_id
    let payload = build_create_payload(file_id, language, soniox_context, false);

    let create_url = format!("{}/transcriptions", base_url());
    let create_resp = common::with_retry(|| {
        let client = client.clone();
        let create_url = create_url.clone();
        let payload = payload.clone();
        async move {
            let resp = client
                .post(&create_url)
                .bearer_auth(key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox create transcription").await)
            }
        }
    })
    .await;
    let create_resp = match create_resp {
        Ok(resp) => resp,
        Err(e) => return (None, Err(e)),
    };
    let create_json: serde_json::Value = match create_resp.json().await {
        Ok(v) => v,
        Err(_) => return (None, Err(common::SttError::BadResponse)),
    };
    let transcription_id = match create_json.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return (None, Err(common::SttError::BadResponse)),
    };

    let result = async {
        // 3) Poll status
        let status_url = format!("{}/transcriptions/{}", base_url(), transcription_id);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(180);
        loop {
            let resp = common::with_retry(|| {
                let client = client.clone();
                let status_url = status_url.clone();
                async move {
                    let resp = client
                        .get(&status_url)
                        .bearer_auth(key)
                        .send()
                        .await
                        .map_err(|e| common::classify_reqwest_err(&e))?;
                    if resp.status().is_success() {
                        Ok(resp)
                    } else {
                        Err(common::log_http_body(resp, "Soniox status").await)
                    }
                }
            })
            .await?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match status {
                "completed" => break,
                "error" => {
                    log::warn!("Soniox transcription job failed");
                    return Err(common::SttError::Server);
                }
                _ => {
                    if started.elapsed() > timeout {
                        return Err(common::SttError::Timeout);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }

        // 4) Fetch transcript
        let transcript_url = format!(
            "{}/transcriptions/{}/transcript",
            base_url(),
            transcription_id
        );
        let resp = common::with_retry(|| {
            let client = client.clone();
            let transcript_url = transcript_url.clone();
            async move {
                let resp = client
                    .get(&transcript_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_http_body(resp, "Soniox transcript").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;

        // Prefer direct text if present, else join tokens
        if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            return Ok(text.to_string());
        }
        if let Some(tokens) = json.get("tokens").and_then(|v| v.as_array()) {
            let mut out = String::new();
            let mut first = true;
            for t in tokens {
                if let Some(txt) = t.get("text").and_then(|v| v.as_str()) {
                    if !first {
                        out.push(' ');
                    } else {
                        first = false;
                    }
                    out.push_str(txt);
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
        Err(common::SttError::BadResponse)
    }
    .await;

    (Some(transcription_id), result)
}

pub(super) async fn transcribe_typed_diarized(
    app: &AppHandle,
    key: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let client = common::http_client();
    let soniox_context = load_soniox_context(app, language);

    let result = transcribe_diarized_with_autoheal(
        &client,
        key,
        wav_path,
        wav_bytes,
        language,
        soniox_context,
    )
    .await;
    if matches!(result, Err(common::SttError::LimitExceeded)) {
        notify_storage_limit(app).await;
    }
    result
}

/// Diarized twin of [`transcribe_typed_with_autoheal`] — same storage-limit
/// self-heal: background cleanup, one full restart from upload.
async fn transcribe_diarized_with_autoheal(
    client: &reqwest::Client,
    key: &str,
    wav_path: &Path,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<super::CloudTranscript, common::SttError> {
    const LIMIT_ATTEMPTS: usize = 2;
    for attempt in 1..=LIMIT_ATTEMPTS {
        match attempt_diarized_once(
            client,
            key,
            wav_path,
            &wav_bytes,
            language,
            soniox_context.clone(),
        )
        .await
        {
            Ok(transcript) => return Ok(transcript),
            Err(e) if matches!(e, common::SttError::LimitExceeded) && attempt < LIMIT_ATTEMPTS => {
                spawn_auto_cleanup(client.clone(), key.to_string());
                wait_for_cleanup_progress(std::time::Duration::from_secs(8)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("attempt loop always returns within LIMIT_ATTEMPTS")
}

/// One full diarized attempt: upload + create/poll/extract + record cleanup.
async fn attempt_diarized_once(
    client: &reqwest::Client,
    key: &str,
    wav_path: &Path,
    wav_bytes: &[u8],
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<super::CloudTranscript, common::SttError> {
    use reqwest::multipart::{Form, Part};

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", base_url());
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.to_vec();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);
            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox upload (diarized)").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();

    let (transcription_id, result) =
        run_diarized_transcription(client, key, &file_id, language, soniox_context).await;

    cleanup_stored_records(client, key, transcription_id.as_deref(), &file_id).await;
    result
}

/// Steps 2-4 of the diarized flow; mirrors [`run_typed_transcription`] but
/// requests speaker diarization and extracts per-word speaker data.
async fn run_diarized_transcription(
    client: &reqwest::Client,
    key: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (
    Option<String>,
    Result<super::CloudTranscript, common::SttError>,
) {
    // 2) Create transcription with diarization -> transcription_id
    let payload = build_create_payload(file_id, language, soniox_context, true);

    let create_url = format!("{}/transcriptions", base_url());
    let create_resp = common::with_retry(|| {
        let client = client.clone();
        let create_url = create_url.clone();
        let payload = payload.clone();
        async move {
            let resp = client
                .post(&create_url)
                .bearer_auth(key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_http_body(resp, "Soniox create transcription (diarized)").await)
            }
        }
    })
    .await;
    let create_resp = match create_resp {
        Ok(resp) => resp,
        Err(e) => return (None, Err(e)),
    };
    let create_json: serde_json::Value = match create_resp.json().await {
        Ok(v) => v,
        Err(_) => return (None, Err(common::SttError::BadResponse)),
    };
    let transcription_id = match create_json.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return (None, Err(common::SttError::BadResponse)),
    };

    let result = async {
        // 3) Poll status
        let status_url = format!("{}/transcriptions/{}", base_url(), transcription_id);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(180);
        loop {
            let resp = common::with_retry(|| {
                let client = client.clone();
                let status_url = status_url.clone();
                async move {
                    let resp = client
                        .get(&status_url)
                        .bearer_auth(key)
                        .send()
                        .await
                        .map_err(|e| common::classify_reqwest_err(&e))?;
                    if resp.status().is_success() {
                        Ok(resp)
                    } else {
                        Err(common::log_http_body(resp, "Soniox status (diarized)").await)
                    }
                }
            })
            .await?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match status {
                "completed" => break,
                "error" => {
                    log::warn!("Soniox diarized transcription job failed");
                    return Err(common::SttError::Server);
                }
                _ => {
                    if started.elapsed() > timeout {
                        return Err(common::SttError::Timeout);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }

        // 4) Fetch transcript
        let transcript_url = format!(
            "{}/transcriptions/{}/transcript",
            base_url(),
            transcription_id
        );
        let resp = common::with_retry(|| {
            let client = client.clone();
            let transcript_url = transcript_url.clone();
            async move {
                let resp = client
                    .get(&transcript_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_http_body(resp, "Soniox transcript (diarized)").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;

        // Extract text (prefer top-level `text`, else join tokens)
        let text = json
            .get("text")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                json.get("tokens").and_then(|v| v.as_array()).map(|tokens| {
                    tokens
                        .iter()
                        .filter_map(|t| t.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            })
            .filter(|s| !s.is_empty())
            .ok_or(common::SttError::BadResponse)?;

        // Parse per-word speaker data from tokens
        let words = json
            .get("tokens")
            .and_then(|v| v.as_array())
            .map(|tokens| tokens.iter().filter_map(parse_soniox_token).collect())
            .unwrap_or_default();

        Ok(super::CloudTranscript { text, words })
    }
    .await;

    (Some(transcription_id), result)
}

fn parse_soniox_token(t: &serde_json::Value) -> Option<crate::transcription::TranscriptionWord> {
    let text = t.get("text").and_then(|v| v.as_str())?.to_string();
    let start_ms = t
        .get("start_ms")
        .and_then(|v| v.as_i64())
        .map(|ms| ms as u64);
    let end_ms = t.get("end_ms").and_then(|v| v.as_i64()).map(|ms| ms as u64);
    let speaker_id = t.get("speaker").and_then(|v| {
        v.as_i64()
            .map(|n| format!("Speaker {n}"))
            .or_else(|| v.as_str().map(|s| format!("Speaker {s}")))
    });
    Some(crate::transcription::TranscriptionWord {
        text,
        start_ms,
        end_ms,
        speaker_id,
        confidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::{SonioxContext, SonioxContextField};

    #[test]
    fn create_payload_includes_language_and_structured_context() {
        let payload = build_create_payload(
            "file_123",
            Some(" en "),
            Some(SonioxContext {
                general: vec![SonioxContextField {
                    key: "domain".to_string(),
                    value: "Software".to_string(),
                }],
                terms: vec!["Voicetypr".to_string(), "Tauri".to_string()],
                text: Some(
                    "Spoken forms map to canonical spellings: voice typer -> Voicetypr."
                        .to_string(),
                ),
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert_eq!(payload["file_id"].as_str(), Some("file_123"));
        assert_eq!(
            payload["language_hints"].as_array().unwrap()[0].as_str(),
            Some("en")
        );
        assert_eq!(
            payload["context"]["terms"].as_array().unwrap()[0].as_str(),
            Some("Voicetypr")
        );
        assert_eq!(
            payload["context"]["text"].as_str(),
            Some("Spoken forms map to canonical spellings: voice typer -> Voicetypr.")
        );
    }

    #[test]
    fn create_payload_omits_empty_optional_fields() {
        let payload = build_create_payload(
            "file_123",
            Some(" "),
            Some(SonioxContext {
                general: Vec::new(),
                terms: Vec::new(),
                text: None,
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert!(payload.get("language_hints").is_none());
        assert!(payload.get("context").is_none());
    }

    #[test]
    fn build_create_payload_diarize_flag_sets_field() {
        let payload = build_create_payload("fid", None, None, true);
        assert_eq!(payload["enable_speaker_diarization"].as_bool(), Some(true));
        let payload_no_diarize = build_create_payload("fid", None, None, false);
        assert!(payload_no_diarize
            .get("enable_speaker_diarization")
            .is_none());
    }

    #[test]
    fn parse_soniox_token_with_speaker_produces_speaker_id() {
        let t = serde_json::json!({
            "text": "Hello",
            "start_ms": 0,
            "end_ms": 500,
            "speaker": 0
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "Hello");
        assert_eq!(word.start_ms, Some(0));
        assert_eq!(word.end_ms, Some(500));
        assert_eq!(word.speaker_id, Some("Speaker 0".to_string()));
        assert!(word.confidence.is_none());
    }

    #[test]
    fn parse_soniox_token_without_speaker_gives_none_speaker_id() {
        let t = serde_json::json!({
            "text": "world",
            "start_ms": 600,
            "end_ms": 900
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "world");
        assert_eq!(word.speaker_id, None);
    }

    #[test]
    fn parse_soniox_token_string_speaker_is_prefixed() {
        let t = serde_json::json!({
            "text": "yes",
            "start_ms": 100,
            "end_ms": 200,
            "speaker": "A"
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.speaker_id, Some("Speaker A".to_string()));
    }

    #[test]
    fn parse_soniox_token_missing_text_returns_none() {
        let t = serde_json::json!({ "start_ms": 0, "end_ms": 100 });
        assert!(parse_soniox_token(&t).is_none());
    }
    mod flow {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        struct BaseOverrideGuard;

        impl BaseOverrideGuard {
            fn install(server: &MockServer) -> Self {
                set_base_url_override(Some(format!("{}/v1", server.uri())));
                BaseOverrideGuard
            }
        }

        impl Drop for BaseOverrideGuard {
            fn drop(&mut self) {
                set_base_url_override(None);
            }
        }

        fn limit_exceeded_body() -> serde_json::Value {
            serde_json::json!({
                "status_code": 429,
                "error_type": "limit_exceeded",
                "message": "Total file count limit has been exceeded for your organization. Please delete some."
            })
        }

        #[tokio::test]
        async fn typed_flow_deletes_transcription_after_success() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "hello world" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) = run_typed_transcription(&client, "k", "f1", Some("en"), None).await;
            assert_eq!(result.unwrap(), "hello world");
            assert_eq!(tid.as_deref(), Some("t1"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn typed_flow_cleans_up_after_job_error() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "error" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) = run_typed_transcription(&client, "k", "f1", None, None).await;
            assert!(matches!(result, Err(common::SttError::Server)));
            assert_eq!(tid.as_deref(), Some("t1"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn create_limit_exceeded_is_terminal_no_retry_and_deletes_orphan_file() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            // Terminal quota wall: exactly ONE create request proves the
            // limit-429 is classified non-transient (no with_retry re-send).
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(429).set_body_json(limit_exceeded_body()))
                .expect(1)
                .mount(&server)
                .await;
            // No transcription was created, so cleanup must delete the file.
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) = run_typed_transcription(&client, "k", "f1", None, None).await;
            assert!(
                matches!(result, Err(common::SttError::LimitExceeded)),
                "expected LimitExceeded, got {result:?}"
            );
            assert!(tid.is_none());

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn storage_limit_self_heals_via_background_cleanup_and_retry() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            // Attempt 1 upload -> f1; attempt 2 (after auto-cleanup) -> f2:
            // the retry must restart from upload because the background
            // cleanup deletes the just-uploaded orphan file.
            let uploads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let uploads_for_mock = uploads.clone();
            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = uploads_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(201).set_body_json(if n == 0 {
                        serde_json::json!({ "id": "f1" })
                    } else {
                        serde_json::json!({ "id": "f2" })
                    })
                })
                .expect(2)
                .mount(&server)
                .await;
            // Attempt 1 create hits the storage wall; attempt 2 succeeds.
            let creates = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let creates_for_mock = creates.clone();
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = creates_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        ResponseTemplate::new(429).set_body_json(limit_exceeded_body())
                    } else {
                        ResponseTemplate::new(201)
                            .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" }))
                    }
                })
                .expect(2)
                .mount(&server)
                .await;
            // Background auto-cleanup: one old transcription + the attempt-1
            // orphan file (f-old listed alongside f1).
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({ "transcriptions": [ { "id": "t-old" } ] }),
                    ),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-old"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "files": [ { "id": "f-old" }, { "id": "f1" } ] }),
                ))
                .mount(&server)
                .await;
            // f1 is deleted twice — by attempt 1's orphan cleanup and again
            // by the background drain's file listing — both legitimate.
            for (file, expected) in [("f-old", 1), ("f1", 2)] {
                Mock::given(method("DELETE"))
                    .and(path(format!("/v1/files/{file}")))
                    .respond_with(ResponseTemplate::new(204))
                    .expect(expected)
                    .mount(&server)
                    .await;
            }
            // Attempt 2 completes normally.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "healed" })),
                )
                .mount(&server)
                .await;

            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("audio.wav");
            std::fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();

            let text = transcribe_typed_with_autoheal(
                &client,
                "k",
                &wav,
                b"RIFF....WAVEfmt ".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
            assert_eq!(text, "healed");

            // The background cleanup task runs concurrently; give it a
            // moment to finish its deletions before verifying expectations.
            for _ in 0..50 {
                if server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .any(|r| r.method.as_str() == "DELETE" && r.url.path() == "/v1/files/f1")
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            server.verify().await;
        }

        #[tokio::test]
        async fn diarized_flow_deletes_transcription_after_success() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t9", "status": "queued" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t9"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t9/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "hi", "tokens": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t9"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) = run_diarized_transcription(&client, "k", "f1", None, None).await;
            assert_eq!(result.unwrap().text, "hi");
            assert_eq!(tid.as_deref(), Some("t9"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }
        #[tokio::test]
        async fn storage_management_internals_count_list_and_delete() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/count"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "playground": 0, "public_api": 3, "total": 3 }),
                ))
                .expect(1)
                .mount(&server)
                .await;
            // Page 1 carries a cursor; page 2 ends pagination.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [ { "id": "t1" }, { "id": "t2" } ],
                    "next_page_cursor": "c1"
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param("cursor", "c1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [ { "id": "t3" } ],
                    "next_page_cursor": null
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            // 409 = still processing; must surface as SkippedProcessing.
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t2"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t3"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let total = get_total(&client, "k", "transcriptions").await.unwrap();
            assert_eq!(total, 3);

            let ids = list_record_ids(&client, "k", "transcriptions")
                .await
                .unwrap();
            assert_eq!(ids, vec!["t1", "t2", "t3"]);

            for id in &ids {
                let url = format!("{}/transcriptions/{id}", base_url());
                let outcome = delete_one(&client, "k", &url).await.unwrap();
                if id == "t2" {
                    assert!(matches!(outcome, DeleteOutcome::SkippedProcessing));
                } else {
                    assert!(matches!(outcome, DeleteOutcome::Deleted));
                }
            }
            server.verify().await;
        }
    }
}
