//! Remote transcription module
//!
//! This module provides functionality for sharing transcription capabilities
//! between VoiceTypr instances over the network.

pub mod client;
pub mod http;
pub mod lifecycle;
pub mod server;
pub mod settings;
pub mod transcription;

#[cfg(test)]
mod integration_tests;
