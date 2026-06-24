//! Remote transcription module
//!
//! This module provides functionality for sharing transcription capabilities
//! between Voicetypr instances over the network.

pub mod client;
pub mod discovery;
pub mod http;
pub mod lifecycle;
pub mod model_control;
pub mod server;
pub mod settings;
pub mod transcription;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod concurrent_tests;
