//! Media control module for pausing/resuming system media during recording.
//!
//! Uses platform-specific APIs:
//! - macOS: `media-remote` crate (MediaRemote.framework via Perl adapter)
//! - Windows: `windows` crate (GlobalSystemMediaTransportControls)

mod controller;

pub use controller::MediaPauseController;
