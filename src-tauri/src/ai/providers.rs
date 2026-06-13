use super::contract::{AiModel, AiProvider};

pub const PROVIDER_CUSTOM: &str = "custom";

pub fn launch_providers() -> Vec<AiProvider> {
    crate::ai::catalog::launch_providers()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    crate::ai::catalog::recommended_models(provider_id)
}
