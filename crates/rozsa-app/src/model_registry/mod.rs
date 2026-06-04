//! Rust model metadata registry.
//!
//! This module owns generated model metadata and `models.json` metadata merging.
//! Credential resolution, OAuth, and shell-command key expansion intentionally remain outside this
//! first migration slice.
//!
//! Related docs: `docs/model/supported-providers.md`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use rozsa_model::providers::openai_completions::{
    DiscoveredModel, NVIDIA_BASE_URL, list_nvidia_models,
};
use rozsa_model::types::{CacheRetention, StreamOptions, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

const GENERATED_MODELS_JSON: &str =
    include_str!("../../../../packages/ai/src/models.generated.json");
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;
const DEFAULT_MAX_TOKENS: usize = 16_384;

/// Error returned when generated or user-provided model metadata cannot be loaded.
#[derive(Debug, Error)]
pub enum ModelRegistryError {
    #[error("Failed to parse generated models: {0}")]
    GeneratedParse(serde_json::Error),
    #[error("Failed to parse models.json: {0}")]
    ModelsJsonParse(serde_json::Error),
    #[error("Invalid models.json: {0}")]
    InvalidModelsJson(String),
    #[error("Failed to read models.json `{path}`: {message}")]
    ModelsJsonRead { path: String, message: String },
}

/// Per-million-token cost metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(rename = "cacheRead")]
    pub cache_read: f64,
    #[serde(rename = "cacheWrite")]
    pub cache_write: f64,
}

impl Default for RegistryModelCost {
    fn default() -> Self {
        Self {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }
    }
}

/// Model metadata shape shared with the TypeScript registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryModel {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub reasoning: bool,
    #[serde(rename = "thinkingLevelMap", skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<Value>,
    pub input: Vec<String>,
    pub cost: RegistryModelCost,
    #[serde(rename = "contextWindow")]
    pub context_window: usize,
    #[serde(rename = "contextModes", skip_serializing_if = "Option::is_none")]
    pub context_modes: Option<Vec<usize>>,
    #[serde(rename = "maxTokens")]
    pub max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat: Option<Value>,
}

/// Rust registry containing generated, configured, and discovered model metadata.
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models: Vec<RegistryModel>,
    user_configured_model_keys: HashSet<String>,
}

impl ModelRegistry {
    /// Load the checked-in generated model metadata.
    pub fn from_generated() -> Result<Self, ModelRegistryError> {
        Self::from_generated_json(GENERATED_MODELS_JSON)
    }

    /// Load generated metadata and merge an optional `models.json` file.
    pub fn from_generated_with_models_json_path(
        models_json_path: Option<&Path>,
    ) -> Result<Self, ModelRegistryError> {
        let mut registry = Self::from_generated()?;
        if let Some(path) = models_json_path {
            if path
                .try_exists()
                .map_err(|error| ModelRegistryError::ModelsJsonRead {
                    path: path.display().to_string(),
                    message: error.to_string(),
                })?
            {
                registry.apply_models_config_file(path)?;
            }
        }
        Ok(registry)
    }

    /// Load model metadata from generated JSON text.
    pub fn from_generated_json(input: &str) -> Result<Self, ModelRegistryError> {
        Ok(Self {
            models: flatten_generated_models(input)?,
            user_configured_model_keys: HashSet::new(),
        })
    }

    /// Return all merged model metadata.
    pub fn all(&self) -> &[RegistryModel] {
        &self.models
    }

    /// Find a model by provider and model ID.
    pub fn find(&self, provider: &str, model_id: &str) -> Option<&RegistryModel> {
        self.models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
    }

    /// Return whether this model was explicitly configured in `models.json`.
    pub fn is_user_configured(&self, provider: &str, model_id: &str) -> bool {
        self.user_configured_model_keys
            .contains(&model_key(provider, model_id))
    }

    /// Merge a `models.json` document into this registry.
    pub fn apply_models_config_json(&mut self, input: &str) -> Result<(), ModelRegistryError> {
        let input = strip_json_comments(input);
        let config: ModelsConfig =
            serde_json::from_str(&input).map_err(ModelRegistryError::ModelsJsonParse)?;
        self.apply_models_config(config)
    }

    /// Merge a `models.json` file into this registry.
    pub fn apply_models_config_file(&mut self, path: &Path) -> Result<(), ModelRegistryError> {
        let input =
            fs::read_to_string(path).map_err(|error| ModelRegistryError::ModelsJsonRead {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        self.apply_models_config_json(&input)
    }

    /// Discover NVIDIA models when `NVIDIA_API_KEY` is configured and merge them into the registry.
    pub async fn merge_nvidia_models_if_configured(&mut self) -> Result<(), ModelRegistryError> {
        if std::env::var("NVIDIA_API_KEY")
            .ok()
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Ok(());
        }
        let models = list_nvidia_models(&StreamOptions {
            temperature: None,
            max_tokens: None,
            api_key: None,
            transport: Transport::Sse,
            cache_retention: CacheRetention::None,
            session_id: None,
            headers: None,
            timeout_ms: Some(5_000),
            max_retries: Some(0),
            max_retry_delay_ms: None,
            metadata: None,
        })
        .await
        .map_err(|error| {
            ModelRegistryError::InvalidModelsJson(format!("NVIDIA model discovery failed: {error}"))
        })?;
        self.merge_openai_compatible_discovered_models("nvidia", NVIDIA_BASE_URL, models);
        Ok(())
    }

    /// Merge dynamically discovered OpenAI-compatible models for one provider.
    pub fn merge_openai_compatible_discovered_models(
        &mut self,
        provider: &str,
        base_url: &str,
        discovered: Vec<DiscoveredModel>,
    ) {
        let models = discovered
            .into_iter()
            .map(|model| RegistryModel {
                id: model.id,
                name: model.name,
                api: "openai-completions".to_string(),
                provider: provider.to_string(),
                base_url: base_url.to_string(),
                reasoning: false,
                thinking_level_map: None,
                input: vec!["text".to_string()],
                cost: RegistryModelCost::default(),
                context_window: model.context_window,
                context_modes: None,
                max_tokens: model.max_tokens,
                headers: None,
                compat: nvidia_openai_compat(provider, base_url),
            })
            .collect::<Vec<_>>();
        self.merge_models(models);
    }

    fn apply_models_config(&mut self, config: ModelsConfig) -> Result<(), ModelRegistryError> {
        self.validate_config(&config)?;
        let built_in_providers = self.provider_ids();
        let mut provider_overrides = HashMap::new();
        let mut model_overrides = HashMap::new();
        let mut custom_models = Vec::new();
        self.user_configured_model_keys.clear();

        for (provider_name, provider_config) in config.providers {
            if provider_config.base_url.is_some() || provider_config.compat.is_some() {
                provider_overrides.insert(
                    provider_name.clone(),
                    ProviderOverride {
                        base_url: provider_config.base_url.clone(),
                        compat: provider_config.compat.clone(),
                    },
                );
            }
            if let Some(overrides) = provider_config.model_overrides.clone() {
                model_overrides.insert(provider_name.clone(), overrides);
            }

            let built_in_defaults = if built_in_providers.contains(&provider_name) {
                self.models
                    .iter()
                    .find(|model| model.provider == provider_name)
                    .map(|model| (model.api.clone(), model.base_url.clone()))
            } else {
                None
            };
            let model_defs = provider_config.models.clone().unwrap_or_default();
            for model_def in model_defs {
                let Some(api) = model_def
                    .api
                    .clone()
                    .or_else(|| provider_config.api.clone())
                    .or_else(|| built_in_defaults.as_ref().map(|value| value.0.clone()))
                else {
                    continue;
                };
                let Some(base_url) = model_def
                    .base_url
                    .clone()
                    .or_else(|| provider_config.base_url.clone())
                    .or_else(|| built_in_defaults.as_ref().map(|value| value.1.clone()))
                else {
                    continue;
                };
                self.user_configured_model_keys
                    .insert(model_key(&provider_name, &model_def.id));
                custom_models.push(model_from_definition(
                    &provider_name,
                    &provider_config,
                    model_def,
                    api,
                    base_url,
                ));
            }
        }

        self.apply_provider_overrides(provider_overrides);
        self.apply_model_overrides(model_overrides);
        self.merge_models(custom_models);
        Ok(())
    }

    fn validate_config(&self, config: &ModelsConfig) -> Result<(), ModelRegistryError> {
        let built_in_providers = self.provider_ids();
        for (provider_name, provider_config) in &config.providers {
            let is_built_in = built_in_providers.contains(provider_name);
            let has_models = provider_config
                .models
                .as_ref()
                .is_some_and(|models| !models.is_empty());
            let has_model_overrides = provider_config
                .model_overrides
                .as_ref()
                .is_some_and(|overrides| !overrides.is_empty());

            if !has_models {
                if provider_config.base_url.is_none()
                    && provider_config.headers.is_none()
                    && provider_config.compat.is_none()
                    && !has_model_overrides
                {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}: must specify baseUrl, headers, compat, modelOverrides, or models."
                    )));
                }
                continue;
            }

            if !is_built_in {
                if provider_config.base_url.is_none() {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}: baseUrl is required when defining custom models."
                    )));
                }
                if provider_config.api_key.is_none() {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}: apiKey is required when defining custom models."
                    )));
                }
            }

            for model in provider_config.models.as_deref().unwrap_or_default() {
                if provider_config.api.is_none() && model.api.is_none() && !is_built_in {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}, model {}: no api specified.",
                        model.id
                    )));
                }
                if model.context_window.is_some_and(|value| value == 0) {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}, model {}: invalid contextWindow.",
                        model.id
                    )));
                }
                if model.max_tokens.is_some_and(|value| value == 0) {
                    return Err(ModelRegistryError::InvalidModelsJson(format!(
                        "Provider {provider_name}, model {}: invalid maxTokens.",
                        model.id
                    )));
                }
            }
        }
        Ok(())
    }

    fn provider_ids(&self) -> HashSet<String> {
        self.models
            .iter()
            .map(|model| model.provider.clone())
            .collect()
    }

    fn apply_provider_overrides(&mut self, overrides: HashMap<String, ProviderOverride>) {
        for model in &mut self.models {
            if let Some(override_config) = overrides.get(&model.provider) {
                if let Some(base_url) = &override_config.base_url {
                    model.base_url = base_url.clone();
                }
                model.compat = merge_compat(model.compat.clone(), override_config.compat.clone());
            }
        }
    }

    fn apply_model_overrides(
        &mut self,
        overrides: HashMap<String, HashMap<String, ModelOverride>>,
    ) {
        for model in &mut self.models {
            if let Some(provider_overrides) = overrides.get(&model.provider) {
                if let Some(model_override) = provider_overrides.get(&model.id) {
                    apply_model_override(model, model_override);
                }
            }
        }
    }

    fn merge_models(&mut self, custom_models: Vec<RegistryModel>) {
        for custom_model in custom_models {
            if let Some(existing) = self.models.iter_mut().find(|model| {
                model.provider == custom_model.provider && model.id == custom_model.id
            }) {
                *existing = custom_model;
            } else {
                self.models.push(custom_model);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModelsConfig {
    providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderConfig {
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
    api: Option<String>,
    headers: Option<HashMap<String, String>>,
    compat: Option<Value>,
    #[serde(default)]
    models: Option<Vec<ModelDefinition>>,
    #[serde(rename = "modelOverrides")]
    model_overrides: Option<HashMap<String, ModelOverride>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelDefinition {
    id: String,
    name: Option<String>,
    api: Option<String>,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    reasoning: Option<bool>,
    #[serde(rename = "thinkingLevelMap")]
    thinking_level_map: Option<Value>,
    input: Option<Vec<String>>,
    cost: Option<RegistryModelCost>,
    #[serde(rename = "contextWindow")]
    context_window: Option<usize>,
    #[serde(rename = "contextModes")]
    context_modes: Option<Vec<usize>>,
    #[serde(rename = "maxTokens")]
    max_tokens: Option<usize>,
    headers: Option<HashMap<String, String>>,
    compat: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelOverride {
    name: Option<String>,
    reasoning: Option<bool>,
    #[serde(rename = "thinkingLevelMap")]
    thinking_level_map: Option<Value>,
    input: Option<Vec<String>>,
    cost: Option<PartialModelCost>,
    #[serde(rename = "contextWindow")]
    context_window: Option<usize>,
    #[serde(rename = "contextModes")]
    context_modes: Option<Vec<usize>>,
    #[serde(rename = "maxTokens")]
    max_tokens: Option<usize>,
    headers: Option<HashMap<String, String>>,
    compat: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct PartialModelCost {
    input: Option<f64>,
    output: Option<f64>,
    #[serde(rename = "cacheRead")]
    cache_read: Option<f64>,
    #[serde(rename = "cacheWrite")]
    cache_write: Option<f64>,
}

#[derive(Debug, Clone)]
struct ProviderOverride {
    base_url: Option<String>,
    compat: Option<Value>,
}

fn flatten_generated_models(input: &str) -> Result<Vec<RegistryModel>, ModelRegistryError> {
    let providers: HashMap<String, HashMap<String, RegistryModel>> =
        serde_json::from_str(input).map_err(ModelRegistryError::GeneratedParse)?;
    Ok(providers
        .into_values()
        .flat_map(|models| models.into_values())
        .collect())
}

fn model_from_definition(
    provider_name: &str,
    provider_config: &ProviderConfig,
    model_def: ModelDefinition,
    api: String,
    base_url: String,
) -> RegistryModel {
    let compat = merge_compat(provider_config.compat.clone(), model_def.compat);
    RegistryModel {
        id: model_def.id.clone(),
        name: model_def.name.unwrap_or(model_def.id),
        api,
        provider: provider_name.to_string(),
        base_url,
        reasoning: model_def.reasoning.unwrap_or(false),
        thinking_level_map: model_def.thinking_level_map,
        input: model_def.input.unwrap_or_else(|| vec!["text".to_string()]),
        cost: model_def.cost.unwrap_or_default(),
        context_window: model_def.context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW),
        context_modes: model_def.context_modes,
        max_tokens: model_def.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        headers: model_def.headers,
        compat,
    }
}

fn apply_model_override(model: &mut RegistryModel, model_override: &ModelOverride) {
    if let Some(name) = &model_override.name {
        model.name = name.clone();
    }
    if let Some(reasoning) = model_override.reasoning {
        model.reasoning = reasoning;
    }
    if let Some(thinking_level_map) = &model_override.thinking_level_map {
        model.thinking_level_map = Some(merge_json_objects(
            model.thinking_level_map.clone(),
            Some(thinking_level_map.clone()),
        ));
    }
    if let Some(input) = &model_override.input {
        model.input = input.clone();
    }
    if let Some(cost) = &model_override.cost {
        if let Some(input) = cost.input {
            model.cost.input = input;
        }
        if let Some(output) = cost.output {
            model.cost.output = output;
        }
        if let Some(cache_read) = cost.cache_read {
            model.cost.cache_read = cache_read;
        }
        if let Some(cache_write) = cost.cache_write {
            model.cost.cache_write = cache_write;
        }
    }
    if let Some(context_window) = model_override.context_window {
        model.context_window = context_window;
    }
    if let Some(context_modes) = &model_override.context_modes {
        model.context_modes = Some(context_modes.clone());
    }
    if let Some(max_tokens) = model_override.max_tokens {
        model.max_tokens = max_tokens;
    }
    if let Some(headers) = &model_override.headers {
        model.headers = Some(headers.clone());
    }
    model.compat = merge_compat(model.compat.clone(), model_override.compat.clone());
}

fn merge_compat(base: Option<Value>, override_value: Option<Value>) -> Option<Value> {
    match (base, override_value) {
        (base, None) => base,
        (Some(Value::Object(mut base)), Some(Value::Object(mut override_object))) => {
            merge_nested_object(&mut base, &mut override_object, "openRouterRouting");
            merge_nested_object(&mut base, &mut override_object, "vercelGatewayRouting");
            for (key, value) in override_object {
                base.insert(key, value);
            }
            Some(Value::Object(base))
        }
        (_, Some(value)) => Some(value),
    }
}

fn merge_json_objects(base: Option<Value>, override_value: Option<Value>) -> Value {
    match merge_compat(base, override_value) {
        Some(value) => value,
        None => Value::Object(Map::new()),
    }
}

fn merge_nested_object(
    base: &mut Map<String, Value>,
    override_object: &mut Map<String, Value>,
    key: &str,
) {
    let Some(override_nested) = override_object.remove(key) else {
        return;
    };
    let Some(base_nested) = base.remove(key) else {
        override_object.insert(key.to_string(), override_nested);
        return;
    };
    override_object.insert(
        key.to_string(),
        merge_json_objects(Some(base_nested), Some(override_nested)),
    );
}

fn nvidia_openai_compat(provider: &str, base_url: &str) -> Option<Value> {
    if provider != "nvidia" && !base_url.contains("integrate.api.nvidia.com") {
        return None;
    }
    Some(serde_json::json!({
        "supportsStore": false,
        "supportsDeveloperRole": false,
        "supportsReasoningEffort": false,
        "maxTokensField": "max_tokens"
    }))
}

fn model_key(provider: &str, model_id: &str) -> String {
    format!("{provider}/{model_id}")
}

fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for comment_ch in chars.by_ref() {
                if comment_ch == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    strip_trailing_commas(&output)
}

fn strip_trailing_commas(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while matches!(lookahead.peek(), Some(next) if next.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('}' | ']')) {
                continue;
            }
        }

        output.push(ch);
    }

    output
}
