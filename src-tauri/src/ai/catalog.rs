use super::contract::{AiModel, AiProvider};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct CatalogFile {
    pub providers: Vec<CatalogProvider>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogProvider {
    pub id: String,
    pub label: String,
    pub status: String,
    pub adapter: Option<String>,
    pub namespace: Option<String>,
    pub requires_api_key: bool,
    pub supports_base_url: bool,
    pub supports_reasoning: bool,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogModel {
    pub model_id: String,
    pub label: String,
    pub recommended: bool,
    pub reasoning: bool,
    pub context: Option<u64>,
    pub cost_input: Option<f64>,
    pub cost_output: Option<f64>,
}

type Catalog = CatalogFile;

// Project rule prefers LazyLock when the initializer is known at declaration time;
// this preserves the contract's parse-once behavior.
static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let mut catalog: Catalog =
        serde_json::from_str(include_str!("../../catalog/catalog.generated.json"))
            .expect("embedded AI provider catalog must be valid JSON");
    catalog.providers.push(CatalogProvider {
        id: "custom".to_string(),
        label: "Custom (OpenAI-compatible)".to_string(),
        status: "production".to_string(),
        adapter: None,
        namespace: None,
        requires_api_key: false,
        supports_base_url: true,
        supports_reasoning: false,
        models: Vec::new(),
    });
    catalog
});

fn catalog() -> &'static Catalog {
    &CATALOG
}

fn provider(provider_id: &str) -> Option<&'static CatalogProvider> {
    catalog()
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
}

fn model(provider_id: &str, model: &CatalogModel) -> AiModel {
    AiModel {
        provider_id: provider_id.to_string(),
        model_id: model.model_id.clone(),
        label: model.label.clone(),
        recommended: model.recommended,
    }
}

pub fn launch_providers() -> Vec<AiProvider> {
    catalog()
        .providers
        .iter()
        .map(|provider| AiProvider {
            id: provider.id.clone(),
            label: provider.label.clone(),
            status: provider.status.clone(),
            requires_api_key: provider.requires_api_key,
            supports_base_url: provider.supports_base_url,
            supports_reasoning: provider.supports_reasoning,
        })
        .collect()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    provider(provider_id)
        .map(|provider| {
            provider
                .models
                .iter()
                .filter(|model| model.recommended)
                .map(|entry| model(provider_id, entry))
                .collect()
        })
        .unwrap_or_default()
}

pub fn all_provider_models(provider_id: &str) -> Vec<&'static CatalogModel> {
    provider(provider_id)
        .map(|provider| provider.models.iter().collect())
        .unwrap_or_default()
}

pub fn is_native_provider(provider_id: &str) -> bool {
    provider(provider_id).is_some_and(|provider| provider.adapter.is_some())
}

pub fn adapter_name(provider_id: &str) -> Option<&'static str> {
    provider(provider_id).and_then(|provider| provider.adapter.as_deref())
}

pub fn namespace(provider_id: &str) -> Option<&'static str> {
    provider(provider_id).and_then(|provider| provider.namespace.as_deref())
}

pub fn provider_for_adapter(adapter_name: &str) -> Option<&'static str> {
    catalog()
        .providers
        .iter()
        .find(|provider| provider.adapter.as_deref() == Some(adapter_name))
        .map(|provider| provider.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_expected_shape() {
        let catalog = catalog();
        let generated_provider_count = catalog
            .providers
            .iter()
            .filter(|provider| provider.id != "custom")
            .count();
        assert!(generated_provider_count >= 3);

        let mut provider_ids = HashSet::new();
        for provider in &catalog.providers {
            assert!(provider_ids.insert(provider.id.as_str()));
            if provider.id != "custom"
                && matches!(provider.status.as_str(), "production" | "experimental")
            {
                assert!(provider
                    .adapter
                    .as_deref()
                    .is_some_and(|adapter| !adapter.is_empty()));
            }

            let mut model_ids = HashSet::new();
            for model in &provider.models {
                assert!(model_ids.insert(model.model_id.as_str()));
            }

            let recommended_ids: HashSet<&str> = provider
                .models
                .iter()
                .filter(|model| model.recommended)
                .map(|model| model.model_id.as_str())
                .collect();
            for recommended in recommended_models(&provider.id) {
                assert!(recommended_ids.contains(recommended.model_id.as_str()));
            }
        }
    }

    #[test]
    fn production_and_experimental_providers_have_adapters() {
        for provider in &catalog().providers {
            if provider.id != "custom"
                && matches!(provider.status.as_str(), "production" | "experimental")
            {
                assert!(
                    adapter_name(&provider.id).is_some(),
                    "{} should have a genai adapter",
                    provider.id
                );
            }
        }
    }

    #[test]
    fn adapter_to_provider_mapping_round_trips() {
        for provider in &catalog().providers {
            if provider.id == "custom" {
                continue;
            }

            let adapter = adapter_name(&provider.id)
                .unwrap_or_else(|| panic!("{} should have a genai adapter", provider.id));
            assert_eq!(provider_for_adapter(adapter), Some(provider.id.as_str()));
        }
    }

    #[test]
    fn recommended_models_are_subset_of_all_models() {
        for provider in &catalog().providers {
            let all_model_ids: HashSet<String> = all_provider_models(&provider.id)
                .into_iter()
                .map(|model| model.model_id.clone())
                .collect();

            for recommended in recommended_models(&provider.id) {
                assert!(
                    all_model_ids.contains(&recommended.model_id),
                    "{} recommended model {} should be in all models",
                    provider.id,
                    recommended.model_id
                );
            }
        }
    }

    #[test]
    fn overlay_recommended_models_exist_in_catalog() {
        let overlay: serde_json::Value =
            serde_json::from_str(include_str!("../../catalog/overlay.json"))
                .expect("overlay must be valid JSON");

        for provider in &catalog().providers {
            let Some(recommended) = overlay
                .get(&provider.id)
                .and_then(|provider| provider.get("recommended"))
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };

            let model_ids: HashSet<&str> = provider
                .models
                .iter()
                .map(|model| model.model_id.as_str())
                .collect();
            for model_id in recommended {
                let model_id = model_id
                    .as_str()
                    .expect("recommended model id must be a string");
                assert!(model_ids.contains(model_id));
            }
        }
    }
}
