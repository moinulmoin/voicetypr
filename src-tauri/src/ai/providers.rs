use super::contract::{AiModel, AiProvider};

pub const PROVIDER_CUSTOM: &str = "custom";
pub const PROVIDER_OPENROUTER: &str = "openrouter";
/// Claude Code (Anthropic's local coding CLI) — subscription-authenticated via
/// the `claude` binary; no API key. Polished via cold-spawn (Phase 4C-i).
pub const PROVIDER_CLAUDE_CODE: &str = "claude-code";

pub fn launch_providers() -> Vec<AiProvider> {
    crate::ai::catalog::launch_providers()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    crate::ai::catalog::recommended_models(provider_id)
}
