//! Manual harness: prints trigger events for a couple of native gestures.
//!
//! Run with: `cargo run -p keytrigger --example print_triggers`
//!
//! macOS: the running process must have Accessibility permission to receive
//! global key events; otherwise the tap fails to create and no events flow
//! (the engine logs and stays idle). Inside the Voicetypr app this permission
//! is already granted.

use std::time::Duration;

use keytrigger::{Modifier, Side, TapKey, Trigger, TriggerEngine};

fn main() {
    env_logger_init();

    let engine = TriggerEngine::new();
    engine.set_bindings(vec![
        (
            "hold-right-option".to_string(),
            Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right,
            },
        ),
        (
            "double-tap-right-command".to_string(),
            Trigger::DoubleTap {
                key: TapKey::Mod(Modifier::Meta, Side::Right),
                within: Duration::from_millis(350),
            },
        ),
    ]);

    engine
        .start(|ev| println!("  -> {} {:?}", ev.id, ev.phase))
        .expect("failed to start engine");

    println!("keytrigger demo running.");
    println!("  - hold Right-Option  -> hold-right-option Pressed/Released");
    println!("  - double-tap Right-Command -> double-tap-right-command Pressed/Released");
    println!("(Requires Accessibility permission. Ctrl-C to quit.)");

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Minimal logger init without pulling an extra dependency: route `log` to
/// stderr via a tiny custom logger.
fn env_logger_init() {
    use log::{Level, LevelFilter, Metadata, Record};

    struct StderrLogger;
    impl log::Log for StderrLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= Level::Info
        }
        fn log(&self, record: &Record) {
            if self.enabled(record.metadata()) {
                eprintln!("[{}] {}", record.level(), record.args());
            }
        }
        fn flush(&self) {}
    }

    static LOGGER: StderrLogger = StderrLogger;
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Info));
}
