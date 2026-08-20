//! Always-on in-memory DEBUG ring (plan 044).
//!
//! tauri-plugin-log owns the global logger and, in release builds, the file
//! sink is filtered at Info — so `log::debug!` reaches no sink in production
//! and bug reports can never explain a failure's inner timings, budgets, or
//! backend decisions. The ring keeps the most recent DEBUG lines in RAM
//! (entry- and byte-capped) and every bug report attaches a redacted dump.
//! There is no user-facing toggle: users cannot get it wrong because there
//! is nothing to get right.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::LazyLock;

use tauri_plugin_log::fern;

const MAX_ENTRIES: usize = 1000;
const MAX_TOTAL_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct State {
    queue: VecDeque<String>,
    bytes: usize,
}

#[derive(Default)]
struct DebugRing {
    state: parking_lot::Mutex<State>,
}

impl DebugRing {
    fn push_line(&self, line: &str) {
        let mut state = self.state.lock();
        state.bytes += line.len();
        state.queue.push_back(line.to_string());
        while state.bytes > MAX_TOTAL_BYTES || state.queue.len() > MAX_ENTRIES {
            let Some(dropped) = state.queue.pop_front() else {
                break;
            };
            state.bytes = state.bytes.saturating_sub(dropped.len());
        }
    }

    /// Most recent lines, oldest first, capped at `max_bytes`.
    fn snapshot_lines(&self, max_bytes: usize) -> Vec<String> {
        let state = self.state.lock();
        let mut out = Vec::new();
        let mut used = 0usize;
        for line in state.queue.iter().rev() {
            used = used.saturating_add(line.len());
            if used > max_bytes {
                break;
            }
            out.push(line.clone());
        }
        out.reverse();
        out
    }
}

static RING: LazyLock<DebugRing> = LazyLock::new(DebugRing::default);

/// Fern sink: receives every formatted record that passes the global level
/// filter. The fern formatter emits one line per record (trailing newline
/// trimmed here); embedded newlines in a message stay on one ring line.
struct RingWriter;

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let line = String::from_utf8_lossy(buf);
        RING.push_line(line.trim_end_matches(['\r', '\n']));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The dispatch handed to `tauri_plugin_log::TargetKind::Dispatch` — the
/// only release-build sink that sees DEBUG records.
pub fn ring_dispatch() -> fern::Dispatch {
    fern::Dispatch::new().chain(Box::new(RingWriter) as Box<dyn Write + Send>)
}
pub fn snapshot_joined(max_bytes: usize) -> String {
    RING.snapshot_lines(max_bytes).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring_with(lines: &[&str]) -> DebugRing {
        let ring = DebugRing::default();
        for line in lines {
            ring.push_line(line);
        }
        ring
    }

    #[test]
    fn ring_keeps_most_recent_lines_under_entry_cap() {
        let lines: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("line-{i}")).collect();
        let ring = DebugRing::default();
        for line in &lines {
            ring.push_line(line);
        }

        let snapshot = ring.snapshot_lines(usize::MAX);
        assert_eq!(snapshot.len(), MAX_ENTRIES, "one over the cap must drop");
        assert_eq!(snapshot.first().map(String::as_str), Some("line-1"));
        assert_eq!(snapshot.last().map(String::as_str), Some("line-1000"));
    }

    #[test]
    fn ring_enforces_total_byte_cap() {
        let big = "x".repeat(600);
        let ring = DebugRing::default();
        for _ in 0..1000 {
            ring.push_line(&big);
        }

        let state = ring.state.lock();
        assert!(state.bytes <= MAX_TOTAL_BYTES);
        assert!(state.queue.len() < 1000, "byte cap must evict entries");
    }

    #[test]
    fn snapshot_byte_cap_takes_most_recent_tail() {
        let ring = ring_with(&["a", "b", "c", "d"]);
        let snapshot = ring.snapshot_lines(2);
        assert_eq!(snapshot, vec!["c", "d"], "oldest lines drop first");
    }
}
