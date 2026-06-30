use std::panic::{catch_unwind, AssertUnwindSafe};

/// Run a monitor query and return `None` if it panics on a stale monitor handle.
///
/// tao 0.34.5 builds each `Monitor` by calling `GetMonitorInfoW().unwrap()`,
/// which panics with `Os { code: 1461 }` (ERROR_INVALID_MONITOR_HANDLE) when a
/// monitor handle goes stale mid display-change (dock / undock / sleep). tauri
/// runs that conversion *inside* `current_monitor()` / `primary_monitor()` /
/// `available_monitors()` (tauri-runtime-wry `From<MonitorHandleWrapper>`), so
/// the panic surfaces during the query — not at `Monitor::size()`, which only
/// returns an already-cached field. Wrapping the whole query + field read lets a
/// transient stale handle fall back gracefully instead of crashing the app.
/// (`profile.release` is `panic = "unwind"`, so the catch is effective.)
pub fn catch_monitor_panic<T>(query: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(query)).ok()
}
