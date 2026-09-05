//! Soniox cloud STT: async Files + Transcriptions + poll flow.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

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

// --- Active-job ownership and drain gating ------------------------------------
//
// Backlog drains (settings action + storage-wall auto-heal) must never delete
// the records of in-flight dictation flows: Soniox documents that deleting a
// file still referenced by a not-yet-processing transcription fails that job
// with `file_not_found`, and a queued/completed-but-not-yet-extracted
// transcription can be deleted server-side, which would kill a healthy
// dictation mid-flight.
//
// Two primitives make that decision race-free:
//
//   * LISTING_GATE — a short-lived async gate. A flow holds a READ guard only
//     across its upload request (until the file id is registered) and across
//     its create request (until the transcription id is attached). This
//     closes the visibility window: a record becomes server-side list-visible
//     BEFORE its id reaches the client, so a drain listing taken while an id
//     is still in flight could see the record without the registration. A
//     drain takes a WRITE guard only around each listing + protection
//     snapshot — writes wait for in-flight upload/create windows, and flows
//     starting afterwards list absent records. Never held across a poll or
//     a whole dictation.
//   * ACTIVE_JOBS — RAII registry of app-owned uploads (file_id →
//     Option<transcription_id>). Drains snapshot it under the gate and skip
//     protected records in BOTH passes. Guards deregister on drop, so a
//     cancelled dictation cannot leak protection; its leftovers fall back to
//     ordinary backlog ownership and the next drain reaps them.

static LISTING_GATE: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

static ACTIVE_JOBS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Option<String>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// RAII ownership of one flow's uploaded file and, once created, its
/// transcription. Dropping the guard releases the protection — cancellation
/// cannot leak it.
struct ActiveJobGuard(String);

impl ActiveJobGuard {
    fn register(file_id: &str) -> Self {
        ACTIVE_JOBS
            .lock()
            .expect("active-jobs registry poisoned")
            .insert(file_id.to_string(), None);
        Self(file_id.to_string())
    }
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        ACTIVE_JOBS
            .lock()
            .expect("active-jobs registry poisoned")
            .remove(&self.0);
    }
}

/// Attaches a created transcription to its already-registered upload. Called
/// under the gate's read guard inside the create window. If the upload entry
/// is somehow gone, the pair is still recorded so the drain cannot delete
/// either record out from under the flow.
fn attach_transcription(file_id: &str, transcription_id: &str) {
    ACTIVE_JOBS
        .lock()
        .expect("active-jobs registry poisoned")
        .insert(file_id.to_string(), Some(transcription_id.to_string()));
}

/// Snapshot of files owned by in-flight flows. The lock is never held across
/// an await point.
fn active_file_ids() -> std::collections::HashSet<String> {
    ACTIVE_JOBS
        .lock()
        .expect("active-jobs registry poisoned")
        .keys()
        .cloned()
        .collect()
}

/// Snapshot of transcription ids owned by in-flight flows.
fn active_transcription_ids() -> std::collections::HashSet<String> {
    ACTIVE_JOBS
        .lock()
        .expect("active-jobs registry poisoned")
        .values()
        .filter_map(|tid| tid.clone())
        .collect()
}

/// Best-effort cleanup of the Soniox records owned by one dictation flow.
/// Soniox documents that deleting a transcription does NOT cascade to its
/// uploaded file — `DELETE /files/{id}` is a separate call. Therefore:
///
///   * transcription present → delete it; only once the delete lands (2xx)
///     or the record is already gone (404 — e.g. a backlog drain raced us
///     and owns the file too) is the file unreferenced, so delete it too.
///   * 409 → the job is still processing (poll-timeout path). The file is
///     still referenced by that live job — deleting it now would fail the
///     job with `file_not_found` — so BOTH records linger for a later
///     drain. A processing 409 is never permission to delete the file.
///   * no transcription (creation never succeeded) → the file is a pure
///     orphan and is deleted directly.
///
/// A 404 on the file delete itself is success: the goal state is "file
/// gone", whoever deleted it. ANY failed transcription delete — 409, 5xx,
/// rate limit, timeout — retains the file: the record may still exist and
/// reference it, and deleting the file would fail the live job with
/// `file_not_found`. Failures are logged and never fatal; leftover records
/// are reaped by the backlog drains. A landed record delete frees record
/// capacity and counts toward the self-heal's record wake; a 404 frees
/// nothing and must not count.
async fn cleanup_stored_records(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<&str>,
    file_id: &str,
) {
    let Some(tid) = transcription_id else {
        delete_file_best_effort(client, key, file_id).await;
        return;
    };
    let url = format!("{}/transcriptions/{tid}", base_url());
    let attempt = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        delete_one(client, key, &url),
    )
    .await;
    match attempt {
        Ok(Ok(DeleteOutcome::Deleted)) => {
            bump_record_freed_progress();
            delete_file_best_effort(client, key, file_id).await;
        }
        Ok(Ok(DeleteOutcome::AlreadyGone)) => {
            delete_file_best_effort(client, key, file_id).await;
        }
        Ok(Ok(DeleteOutcome::SkippedProcessing)) => {
            log::info!(
                "Soniox cleanup: transcription {tid} still processing; file {file_id} stays until the job finishes"
            );
        }
        Ok(Err(e)) => log::warn!(
            "Soniox cleanup: delete transcription {tid} failed: {e}; file {file_id} left for backlog cleanup"
        ),
        Err(_) => log::warn!("Soniox cleanup: delete transcription timed out ({url})"),
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
    /// Files kept because an in-flight dictation flow owns them or a live
    /// (still-processing) job references them — see the active-job
    /// ownership notes on [`drain_stored_records`].
    pub skipped_active: u64,
    /// Transcription records kept because an in-flight dictation flow owns
    /// them (possibly queued or completed-but-not-yet-extracted).
    pub skipped_active_jobs: u64,
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

/// Minimal typed metadata for a listed transcription: only the fields the
/// drain needs, so no full provider JSON pages are retained while paging
/// through up to 100k records.
#[derive(serde::Deserialize)]
struct ListedTranscription {
    id: String,
    #[serde(default)]
    file_id: Option<String>,
}

/// Minimal typed metadata for a listed file.
#[derive(serde::Deserialize)]
struct ListedFile {
    id: String,
}

/// Cursor-paginated typed listing for a collection ("files" |
/// "transcriptions"). Defensively accepts `items`/`data` array keys; Soniox
/// documents `next_page_cursor` and a 1000-item page cap. Each page decodes
/// item-by-item into the minimal `T` (malformed items are skipped, matching
/// the previous defensive id extraction) and only typed metadata is kept.
async fn list_pages<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
) -> Result<Vec<T>, common::SttError> {
    let mut items = Vec::new();
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
            .or_else(|| json.get("items"))
            .or_else(|| json.get("data"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for item in page {
            if let Ok(typed) = serde_json::from_value::<T>(item) {
                items.push(typed);
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
    Ok(items)
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

/// Drains stored Soniox records so retained files free capacity promptly:
/// each transcription record whose delete lands has its file deleted
/// immediately (transcription deletion does NOT cascade server-side —
/// docs), so the storage-wall self-heal only needs the first delete
/// (~tens of ms) instead of a full backlog pass before upload capacity
/// frees. The final fresh file listing then reaps whatever remains:
/// pre-existing orphans, files whose inline delete was deferred or failed,
/// and files whose owning record no longer exists.
///
/// Both listing passes run under the drain gate's WRITE guard, which makes
/// the protection snapshot exact (see the module-level ownership notes):
/// flows hold READ guards only across upload→register and
/// create→attach, so any record a listing can see is either already
/// registered in ACTIVE_JOBS or was created after the listing and is
/// therefore absent from it. Neither pass deletes:
///   * records/files registered by in-flight dictation flows — including
///     queued or completed-but-not-yet-extracted jobs, whose records the
///     API would happily delete;
///   * files referenced by records whose delete was refused (409 still
///     processing, or a transient failure): the record still exists and
///     its live job still needs the file.
///
/// Pacing: Soniox's file-management RPM is real but its numeric limit is
/// undocumented. Shared by the settings clean-up command (progress events)
/// and the background auto-clean (counter only); only FILE deletions bump
/// the auto-heal progress counter, since file-count/size walls gate the
/// retried upload and transcription deletions free no file capacity.
async fn drain_stored_records(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<SonioxCleanupResult, String> {
    let mut result = SonioxCleanupResult::default();

    // Pass 1 — transcription records. Listing + protection snapshot under
    // the write gate; deletes run outside it.
    let (transcriptions, mut protected_files) = {
        let _drain_gate = LISTING_GATE.write().await;
        let transcriptions = list_pages::<ListedTranscription>(client, key, "transcriptions")
            .await
            .map_err(|e| e.message("Soniox"))?;
        let active_tids = active_transcription_ids();
        let mut protected_files = std::collections::HashSet::new();
        for item in &transcriptions {
            if active_tids.contains(&item.id) {
                if let Some(file_id) = &item.file_id {
                    protected_files.insert(file_id.clone());
                }
            }
        }
        (transcriptions, protected_files)
    };
    let mut total_ops = transcriptions.len() as u64;
    let mut done_ops: u64 = 0;

    for item in &transcriptions {
        // App-owned job — the gate normally keeps these out of the snapshot
        // path entirely; this re-check covers attaches that raced between
        // the pass-1 snapshot and this delete. Never delete the record, and
        // keep its file protected for the file pass.
        if active_transcription_ids().contains(&item.id) {
            result.skipped_active_jobs += 1;
            if let Some(file_id) = &item.file_id {
                protected_files.insert(file_id.clone());
            }
            continue;
        }
        let url = format!("{}/transcriptions/{}", base_url(), item.id);
        match delete_one(client, key, &url).await {
            Ok(DeleteOutcome::SkippedProcessing) => {
                result.skipped_processing += 1;
                if let Some(file_id) = &item.file_id {
                    protected_files.insert(file_id.clone());
                }
            }
            Err(e) => {
                result.errors.push(format!("transcription {}: {e}", item.id));
                // The record still exists and references its file — the
                // file pass must not break the live job.
                if let Some(file_id) = &item.file_id {
                    protected_files.insert(file_id.clone());
                }
            }
            outcome => {
                // Deleted or AlreadyGone: the record is gone, so its file is
                // unreferenced — free it inline so capacity frees without
                // waiting for the whole backlog pass.
                if matches!(outcome, Ok(DeleteOutcome::Deleted)) {
                    result.deleted_transcriptions += 1;
                }
                if let Some(file_id) = &item.file_id {
                    if !active_file_ids().contains(file_id) {
                        let file_url = format!("{}/files/{file_id}", base_url());
                        match delete_one(client, key, &file_url).await {
                            Ok(DeleteOutcome::Deleted) => {
                                result.deleted_files += 1;
                                bump_auto_cleanup_progress();
                            }
                            // AlreadyGone: freed elsewhere, nothing to
                            // report; anything else: leave it for the
                            // pass-2 fresh listing.
                            _ => {}
                        }
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Pass 2 — fresh listing + snapshot under the write gate: pre-existing
    // orphans, files whose inline delete was deferred (still registered by
    // a flow at the time) or failed, all show up here; files already freed
    // inline no longer do.
    let (file_items, protected) = {
        let _drain_gate = LISTING_GATE.write().await;
        let file_items = list_pages::<ListedFile>(client, key, "files")
            .await
            .map_err(|e| e.message("Soniox"))?;
        let mut protected = active_file_ids();
        protected.extend(protected_files);
        (file_items, protected)
    };
    total_ops += file_items.len() as u64;

    for item in &file_items {
        done_ops += 1;
        if protected.contains(&item.id) {
            result.skipped_active += 1;
        } else {
            let url = format!("{}/files/{}", base_url(), item.id);
            match delete_one(client, key, &url).await {
                Ok(DeleteOutcome::Deleted) => {
                    result.deleted_files += 1;
                    bump_auto_cleanup_progress();
                }
                Ok(DeleteOutcome::AlreadyGone) => {}
                // Files have no processing state; a 409 here is unexpected.
                Ok(DeleteOutcome::SkippedProcessing) => {
                    result.errors.push(format!("file {}: HTTP 409", item.id))
                }
                Err(e) => result.errors.push(format!("file {}: {e}", item.id)),
            }
        }
        done_ops += 1;
        if done_ops % 25 == 0 {
            if let Some(report) = report {
                report(done_ops, total_ops);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Pass 2 — fresh listings under the write gate. The fresh transcription
    // listing closes the post-pass-1 gap: jobs created while pass 1 was
    // deleting, whose guard then dropped, still reference their files and
    // appear here. A file is deletable only when no surviving record and no
    // in-flight flow references it.
    if !files_fail_closed {
        let mut protected = protected_files;
        let mut file_items: Vec<ListedFile> = Vec::new();
        {
            let _drain_gate = LISTING_GATE.write().await;
            let (fresh_transcriptions, fresh_skipped) =
                list_pages::<ListedTranscription>(client, key, "transcriptions")
                    .await
                    .map_err(|e| e.message("Soniox"))?;
            for file_id in fresh_transcriptions.iter().filter_map(|t| t.known_file()) {
                protected.insert(file_id.to_string());
            }
            if fresh_skipped > 0 {
                files_fail_closed = true;
                result.errors.push(format!(
                    "{fresh_skipped} transcription records had incomplete metadata; file cleanup skipped"
                ));
            }
            if !files_fail_closed {
                let (items, skipped_files) =
                    list_pages::<ListedFile>(client, key, "files")
                        .await
                        .map_err(|e| e.message("Soniox"))?;
                if skipped_files > 0 {
                    files_fail_closed = true;
                    result.errors.push(format!(
                        "{skipped_files} file records had incomplete metadata; file cleanup skipped"
                    ));
                } else {
                    file_items = items;
                }
            }
            // Registry snapshot under the same gate as the listings.
            protected.extend(active_file_ids());
        }
        if !files_fail_closed {
            total_ops += file_items.len() as u64;
            for item in &file_items {
                done_ops += 1;
                if protected.contains(&item.id) {
                    result.skipped_active += 1;
                } else {
                    let url = format!("{}/files/{}", base_url(), item.id);
                    match delete_one(client, key, &url).await {
                        Ok(DeleteOutcome::Deleted) => {
                            result.deleted_files += 1;
                            bump_auto_cleanup_progress();
                        }
                        Ok(DeleteOutcome::AlreadyGone) => {}
                        // Files have no processing state; a 409 here is
                        // unexpected.
                        Ok(DeleteOutcome::SkippedProcessing) => {
                            result.errors.push(format!("file {}: HTTP 409", item.id))
                        }
                        Err(e) => result.errors.push(format!("file {}: {e}", item.id)),
                    }
                }
                if done_ops % 25 == 0 {
                    if let Some(report) = report {
                        report(done_ops, total_ops);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
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
                "Soniox background cleanup finished: {} transcriptions + {} files deleted, {} processing-skipped, {} active-protected files, {} active-protected jobs, {} errors",
                totals.deleted_transcriptions,
                totals.deleted_files,
                totals.skipped_processing,
                totals.skipped_active,
                totals.skipped_active_jobs,
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
    model: &str,
    file_id: &str,
    language: Option<&str>,
    context: Option<crate::writing::SonioxContext>,
    diarize: bool,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": model,
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
    model: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let client = common::http_client();
    let soniox_context = load_soniox_context(app, language);

    let result = transcribe_typed_with_autoheal(
        &client,
        key,
        model,
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

/// Upload → transcribe → delete-records with the plan-044 storage-limit
/// self-heal: on `LimitExceeded`, a background cleanup drains stored records
/// (each deletion immediately frees capacity); once the first record is gone
/// the WHOLE flow restarts from upload — the just-uploaded file may itself
/// have been deleted by the cleanup, so the create step must not be reused.
/// One retry; a second limit wall is terminal.
async fn transcribe_typed_with_autoheal(
    client: &reqwest::Client,
    key: &str,
    model: &str,
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
            model,
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
    model: &str,
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
    // The gate's read guard spans exactly the window in which the upload
    // becomes server-side list-visible (during the POST) but its id is not
    // yet registered — a drain listing cannot interleave here. Released
    // right after registration; never held across poll/extract.
    let _upload_gate = LISTING_GATE.read().await;
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
    // Own the upload for the rest of this attempt: drains must never see
    // it as unreferenced backlog. Dropped on return or cancellation, so
    // protection cannot leak; unowned leftovers are reaped by later drains.
    let _active_file = ActiveJobGuard::register(&file_id);
    drop(_upload_gate);

    let (transcription_id, result) =
        run_typed_transcription(client, key, model, &file_id, language, soniox_context).await;

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
    model: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (Option<String>, Result<String, common::SttError>) {
    // 2) Create transcription -> transcription_id
    let payload = build_create_payload(model, file_id, language, soniox_context, false);

    let create_url = format!("{}/transcriptions", base_url());
    // Second short gate window: the transcription becomes list-visible
    // during this POST, so the create and the registry attach must complete
    // atomically against any drain's listing + snapshot.
    let transcription_id = {
        let _create_gate = LISTING_GATE.read().await;
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
        attach_transcription(file_id, &transcription_id);
        transcription_id
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
    model: &str,
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
        model,
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
    model: &str,
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
            model,
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
    model: &str,
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
    // Gate window 1 (mirrors the typed flow): upload POST until the file id
    // is registered — a drain listing cannot interleave with the
    // list-visible-before-registered gap.
    let _upload_gate = LISTING_GATE.read().await;
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
    // Own the upload for the rest of this attempt (mirrors the typed flow):
    // drains must never see it as unreferenced backlog, and dropping the
    // guard on return/cancellation cannot leak the protection.
    let _active_file = ActiveJobGuard::register(&file_id);
    drop(_upload_gate);

    let (transcription_id, result) =
        run_diarized_transcription(client, key, model, &file_id, language, soniox_context).await;

    cleanup_stored_records(client, key, transcription_id.as_deref(), &file_id).await;
    result
}

/// Steps 2-4 of the diarized flow; mirrors [`run_typed_transcription`] but
/// requests speaker diarization and extracts per-word speaker data.
async fn run_diarized_transcription(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (
    Option<String>,
    Result<super::CloudTranscript, common::SttError>,
) {
    // 2) Create transcription with diarization -> transcription_id
    let payload = build_create_payload(model, file_id, language, soniox_context, true);

    let create_url = format!("{}/transcriptions", base_url());
    // Gate window 2 (mirrors the typed flow): create POST until the
    // transcription id is attached to the registered upload.
    let transcription_id = {
        let _create_gate = LISTING_GATE.read().await;
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
        attach_transcription(file_id, &transcription_id);
        transcription_id
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
            "stt-async-v5",
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
            "stt-async-v5",
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
        let payload = build_create_payload("stt-async-v5", "fid", None, None, true);
        assert_eq!(payload["enable_speaker_diarization"].as_bool(), Some(true));
        let payload_no_diarize = build_create_payload("stt-async-v5", "fid", None, None, false);
        assert!(payload_no_diarize
            .get("enable_speaker_diarization")
            .is_none());
    }

    #[test]
    fn create_payload_uses_selected_model() {
        let payload = build_create_payload("custom-model", "fid", None, None, false);
        assert_eq!(payload["model"].as_str(), Some("custom-model"));
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

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", Some("en"), None).await;
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

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
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

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
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
            // Background auto-cleanup: one old transcription (carrying its
            // file reference) + the attempt-1 orphan file (f1). The drain
            // frees t-old's file f-old INLINE right after the record delete
            // — capacity frees without waiting for a full backlog pass.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({
                            "transcriptions": [
                                { "id": "t-old", "file_id": "f-old" }
                            ]
                        }),
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
            // Fresh file listing after pass 1: f-old was already freed
            // inline, so only the attempt-1 orphan f1 remains.
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "files": [ { "id": "f1" } ] }),
                ))
                .mount(&server)
                .await;
            // f1 is deleted twice — by attempt 1's orphan cleanup and again
            // by the background drain's file listing — both legitimate. The
            // self-heal wait unblocks on the first FILE deletion (f-old,
            // freed inline in pass 1), the only deletion that frees upload
            // capacity.
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
                "stt-async-v5",
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

            let (tid, result) =
                run_diarized_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
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

            let (items, _skipped) =
                list_pages::<ListedTranscription>(&client, "k", "transcriptions")
                    .await
                    .unwrap();
            let ids: Vec<String> = items.into_iter().map(|item| item.id).collect();
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

        #[tokio::test]
        async fn processing_transcription_409_keeps_its_file() {
            // Poll-timeout path: the job is still processing, so the delete
            // of the transcription 409s. Soniox documents that a file still
            // referenced by a transcription that has not finished processing
            // must NOT be deleted (the job would fail `file_not_found`) — a
            // processing 409 is never permission to delete the active file.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t1"],
                "file delete must not be attempted while the job is processing"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn cleanup_treats_missing_records_as_success() {
            // Racing a drain (or a previous attempt) that already deleted
            // the records must still be a success: 404 is the goal state for
            // both the transcription and the file, and the file delete is
            // still attempted after the transcription 404.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn backlog_drain_never_deletes_active_or_live_job_files() {
            // Storage-wall auto-heal / manual backlog drain: files owned by
            // in-flight dictation flows (registered active) and files still
            // referenced by records that could not be deleted because they
            // are processing (409) must survive the drain; only genuinely
            // unreferenced files are deleted.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            // A transcription still processing (409 on delete) referencing
            // f-live.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "transcriptions": [
                            { "id": "t-live", "status": "transcribing", "file_id": "f-live" }
                        ]
                    }),
                ))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-live"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "files": [
                            { "id": "f-live" },
                            { "id": "f-active" },
                            { "id": "f-orphan" }
                        ]
                    }),
                ))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-orphan"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            // A concurrent dictation flow owns f-active (registered until
            // its guard drops — cancellation cannot leak the protection).
            let _active = ActiveJobGuard::register("f-active");

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.skipped_processing, 1);
            assert_eq!(result.skipped_active, 1);
            assert_eq!(result.deleted_files, 1);
            assert_eq!(result.deleted_transcriptions, 0);
            assert!(result.errors.is_empty(), "{:?}", result.errors);

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t-live", "/v1/files/f-orphan"],
                "active upload and live-job file must not be deleted"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn active_file_protection_ends_when_guard_drops() {
            // Cancellation contract: dropping the flow's guard releases the
            // protection immediately, so the NEXT drain reaps the leftover
            // upload (cleanup ownership returns to the backlog drain).
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "transcriptions": [] })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "files": [ { "id": "f-guard" } ] })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-guard"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let active = ActiveJobGuard::register("f-guard");
            let first = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(first.deleted_files, 0);
            assert_eq!(first.skipped_active, 1);

            drop(active);
            let second = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(second.deleted_files, 1);
            assert_eq!(second.skipped_active, 0);

            server.verify().await;
        }

        #[tokio::test]
        async fn transient_rate_limit_429_stays_rate_limited_without_destructive_drain() {
            // A per-minute file-management RPM wall uses the SAME
            // `error_type: "limit_exceeded"` as retained-storage walls, but
            // its documented message names the per-minute rate. It must
            // surface as the transient RateLimited — NOT trigger the
            // storage self-heal (no whole-library drain, no storage nag).
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(429).set_body_json(
                    serde_json::json!({
                        "status_code": 429,
                        "error_type": "limit_exceeded",
                        "message": "Requests per minute limit for file management has been exceeded for your organization."
                    }),
                ))
                // Exactly two uploads: the shared transient retry, nothing
                // more — no storage-wall restart from upload.
                .expect(2)
                .mount(&server)
                .await;

            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("audio.wav");
            std::fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();

            let error = transcribe_typed_with_autoheal(
                &client,
                "k",
                "stt-async-v5",
                &wav,
                b"RIFF....WAVEfmt ".to_vec(),
                None,
                None,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(error, common::SttError::RateLimited),
                "expected RateLimited, got {error:?}"
            );

            // No DELETE may reach the server: a rate limit must never
            // destroy stored records.
            let deletes = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .count();
            assert_eq!(deletes, 0, "transient 429 must not spawn a cleanup drain");
            server.verify().await;
        }

        #[tokio::test]
        async fn drain_listings_wait_out_in_flight_upload_registration() {
            // Delayed-visibility regression: a file becomes server-side
            // list-visible during the upload POST, BEFORE the response
            // delivers the id and the flow can register it. While an
            // upload window (gate read guard) is open, a drain must not
            // list — otherwise it could see the file unregistered, delete
            // it, and fail the job with `file_not_found`.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "transcriptions": [] })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "files": [ { "id": "f1" } ] })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;

            // Simulate the upload window: gate read guard held, response
            // not yet processed, nothing registered.
            let _upload_window = LISTING_GATE.read().await;
            let drain_client = client.clone();
            let drain = tokio::spawn(async move {
                drain_stored_records(&drain_client, "k", None).await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // The drain must still be parked on the gate: no listing
            // request has been issued while the id was unregistered.
            let listings_so_far = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "GET" && r.url.path() == "/v1/files")
                .count();
            assert_eq!(
                listings_so_far, 0,
                "drain listed files while an upload was still unregistered"
            );

            // The upload response lands and the flow registers the id —
            // exactly what attempt_typed_once does before dropping the
            // gate. Only then may the drain proceed.
            let _registered = ActiveJobGuard::register("f1");
            drop(_upload_window);

            let result = drain.await.unwrap().unwrap();
            assert_eq!(result.skipped_active, 1, "registered upload must survive");
            assert_eq!(result.deleted_files, 0);
        }

        #[tokio::test]
        async fn drain_pass_one_never_deletes_app_owned_jobs() {
            // A queued or completed-but-not-yet-extracted transcription of
            // an in-flight flow can be deleted server-side — pass 1 must
            // skip app-owned records entirely and keep their file
            // protected in pass 2.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "transcriptions": [
                            { "id": "t-app", "status": "queued", "file_id": "f-app" }
                        ]
                    }),
                ))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "files": [ { "id": "f-app" } ] }),
                ))
                .mount(&server)
                .await;

            let _owner = ActiveJobGuard::register("f-app");
            attach_transcription("f-app", "t-app");

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.skipped_active_jobs, 1);
            assert_eq!(result.skipped_active, 1);
            assert_eq!(result.deleted_transcriptions, 0);
            assert_eq!(result.deleted_files, 0);

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert!(
                deletes.is_empty(),
                "app-owned records must not be drained: {deletes:?}"
            );
        }

        #[tokio::test]
        async fn failed_transcription_delete_keeps_file_ref() {
            // Beyond the processing 409: ANY failed record delete (here a
            // transient 500) must retain the file — the record still
            // exists and its job still needs it. Deleting the file on a
            // failed record delete would fail the job with
            // `file_not_found`.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(500))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t1"],
                "file delete must not follow a failed record delete"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn drain_frees_terminal_record_file_without_full_backlog_pass() {
            // Capacity-awareness regression: the self-heal retry waits for
            // FILE capacity, so a terminal record's file must be freed
            // inline in pass 1 — before the (possibly huge) file listing —
            // not deferred until every record has been drained.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server);
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "transcriptions": [
                            { "id": "t-done", "status": "completed", "file_id": "f-done" }
                        ]
                    }),
                ))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-done"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-done"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "files": [] })))
                .mount(&server)
                .await;

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 1);

            // The file delete must have landed BEFORE the pass-2 file
            // listing: inline freeing, not end-of-drain freeing.
            let requests = server.received_requests().await.unwrap();
            let delete_index = requests
                .iter()
                .position(|r| r.method.as_str() == "DELETE" && r.url.path() == "/v1/files/f-done")
                .expect("inline file delete missing");
            let files_list_index = requests
                .iter()
                .position(|r| r.method.as_str() == "GET" && r.url.path() == "/v1/files")
                .expect("pass-2 file listing missing");
            assert!(
                delete_index < files_list_index,
                "file must be freed inline (at {delete_index}) before the pass-2 listing (at {files_list_index})"
            );
            server.verify().await;
        }
    }
}
