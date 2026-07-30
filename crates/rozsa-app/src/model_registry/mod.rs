// FrameworkTree
// mod.rs
// ├── struct ProviderAvailable
// ├── enum ModelRegistryError
// ├── struct RegistryModelCost
// ├── impl RegistryModelCost
// ├── default()
// ├── struct RegistryModel
// ├── impl RegistryModel
// ├── to_model()
// ├── struct RegistryImageModel
// ├── struct ModelRegistry
// ├── struct ImageModelRegistry
// ├── impl ImageModelRegistry
// ├── from_generated_json()
// ├── all()
// ├── all_json()
// ├── find()
// ├── provider_available()
// ├── provider_ids()
// ├── impl ModelRegistry
// ├── load_from_dirs()
// ├── load_from_dir()
// ├── apply_dir()
// ├── from_generated_json()
// ├── all()
// ├── all_json()
// ├── find()
// ├── resolve()
// ├── find_by_id()
// ├── first_available()
// ├── is_user_configured()
// ├── model_config_path()
// ├── persist_thinking_effort()
// ├── apply_models_config_json()
// ├── apply_models_config_file()
// ├── merge_nvidia_models_if_configured()
// ├── merge_openai_compatible_discovered_models()
// ├── apply_models_config()
// ├── apply_models_config_with_source()
// ├── validate_config()
// ├── provider_ids()
// ├── apply_provider_overrides()
// ├── apply_model_overrides()
// ├── merge_models()
// ├── provider_available()
// ├── struct ModelsConfig
// ├── struct ProviderConfig
// ├── struct ModelDefinition
// ├── struct ModelOverride
// ├── struct PartialModelCost
// ├── struct ProviderOverride
// ├── flatten_generated_models()
// ├── flatten_generated_image_models()
// ├── model_from_definition()
// ├── apply_model_override()
// ├── merge_compat()
// ├── merge_json_objects()
// ├── merge_nested_object()
// ├── nvidia_openai_compat()
// ├── provider_from_str()
// ├── is_models_config_path()
// ├── model_key()
// ├── strip_json_comments()
// ├── strip_trailing_commas()
// ├── mod tests
// └── models_config_scan_ignores_auth_json()

//! Rust model metadata registry.
//!
//! This module owns generated model metadata and `models.json` metadata merging.
//! Credential resolution, OAuth, and shell-command key expansion intentionally
//! remain outside this module.
//!
//! Related docs: `docs/model/supported-providers.md`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rozsa_model::env_keys::get_env_api_key;
use rozsa_model::providers::openai_completions::{
    DiscoveredModel, NVIDIA_BASE_URL, list_nvidia_models,
};
use rozsa_model::types::Provider;
use rozsa_model::types::{CacheRetention, StreamOptions, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

const DEFAULT_CONTEXT_WINDOW: usize = 128_000;
const DEFAULT_MAX_TOKENS: usize = 16_384;

/// Auth availability for a single provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderAvailable {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Error returned when generated or user-provided model metadata cannot be loaded.
#[derive(Debug, Error)]
pub enum ModelRegistryError {
    #[error("Failed to parse generated models: {0}")]
    GeneratedParse(serde_json::Error),
    #[error("Failed to parse generated image models: {0}")]
    GeneratedImageParse(serde_json::Error),
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

/// Serialized model metadata shape used while reading configured registries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RegistryModel {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub reasoning: bool,
    #[serde(
        rename = "thinkingEffortMap",
        alias = "thinkingLevelMap",
        skip_serializing_if = "Option::is_none"
    )]
    pub thinking_effort_map: Option<Value>,
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

impl RegistryModel {
    /// Convert to the runtime Model type used by rozsa-model/rozsa-core.
    pub fn to_model(&self) -> rozsa_model::types::Model {
        use rozsa_model::types::{Api, InputModality, Model, ModelCost, Provider};

        let api = match self.api.as_str() {
            "anthropic-messages" => Api::AnthropicMessages,
            "openai-completions" => Api::OpenAICompletions,
            "openai-responses" => Api::OpenAIResponses,
            "bedrock-converse-stream" => Api::BedrockConverseStream,
            "google-generative-ai" => Api::GoogleGenerativeAI,
            "google-vertex" => Api::GoogleVertex,
            "mistral-conversations" => Api::MistralConversations,
            other => Api::Custom(other.to_string()),
        };

        let provider = match self.provider.as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "amazon-bedrock" => Provider::AmazonBedrock,
            "google" => Provider::Google,
            "google-vertex" => Provider::GoogleVertex,
            "deepseek" => Provider::DeepSeek,
            "openrouter" => Provider::OpenRouter,
            "xai" => Provider::XAI,
            "groq" => Provider::Groq,
            "cerebras" => Provider::Cerebras,
            "mistral" => Provider::Mistral,
            "nvidia" => Provider::Nvidia,
            "zai" => Provider::Zai,
            "together" => Provider::Together,
            "moonshot-ai" => Provider::MoonshotAI,
            "moonshot-ai-cn" => Provider::MoonshotAICn,
            "huggingface" => Provider::HuggingFace,
            "cloudflare-workers-ai" => Provider::CloudflareWorkersAI,
            "cloudflare-ai-gateway" => Provider::CloudflareAIGateway,
            "xiaomi" => Provider::Xiaomi,
            other => Provider::Custom(other.to_string()),
        };

        let input_modalities = self
            .input
            .iter()
            .filter_map(|s| match s.as_str() {
                "text" => Some(InputModality::Text),
                "image" => Some(InputModality::Image),
                _ => None,
            })
            .collect();

        Model {
            id: self.id.clone(),
            name: self.name.clone(),
            api,
            provider,
            base_url: self.base_url.clone(),
            reasoning: self.reasoning,
            input_modalities,
            cost: ModelCost {
                input: self.cost.input,
                output: self.cost.output,
                cache_read: self.cost.cache_read,
                cache_write: self.cost.cache_write,
            },
            context_window: self.context_window,
            max_tokens: self.max_tokens,
            thinking_effort_map: self
                .thinking_effort_map
                .clone()
                .and_then(|map| serde_json::from_value(map).ok()),
            headers: self.headers.clone(),
            compat: self.compat.clone(),
        }
    }
}

/// Serialized image-model metadata shape used while reading configured registries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryImageModel {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub input: Vec<String>,
    pub output: Vec<String>,
    pub cost: RegistryModelCost,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

/// Rust registry containing generated, configured, and discovered model metadata.
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models: Vec<RegistryModel>,
    user_configured_model_keys: HashSet<String>,
    model_config_paths: HashMap<String, PathBuf>,
    /// Provider-level apiKey from models.json (raw value, not resolved).
    provider_api_keys: HashMap<String, String>,
}

/// Rust registry containing generated image model metadata.
#[derive(Debug, Clone)]
pub struct ImageModelRegistry {
    models: Vec<RegistryImageModel>,
}

impl ImageModelRegistry {
    /// Load image model metadata from generated JSON text.
    pub fn from_generated_json(input: &str) -> Result<Self, ModelRegistryError> {
        Ok(Self {
            models: flatten_generated_image_models(input)?,
        })
    }

    /// Return all image model entries.
    pub fn all(&self) -> &[RegistryImageModel] {
        &self.models
    }

    /// Return all image model metadata as JSON for frontend IPC.
    pub fn all_json(&self) -> serde_json::Value {
        serde_json::json!(&self.models)
    }

    /// Find an image model by provider and model ID.
    pub fn find(&self, provider: &str, model_id: &str) -> Option<&RegistryImageModel> {
        self.models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
    }

    /// Compute auth availability per image provider using known env vars.
    pub fn provider_available(&self) -> HashMap<String, ProviderAvailable> {
        let mut result = HashMap::new();
        for provider_name in self.provider_ids() {
            let provider_enum = provider_from_str(&provider_name);
            let configured = provider_enum.as_ref().and_then(get_env_api_key).is_some();
            result.insert(
                provider_name,
                ProviderAvailable {
                    configured,
                    source: configured.then(|| "environment".to_string()),
                },
            );
        }
        result
    }

    fn provider_ids(&self) -> HashSet<String> {
        self.models
            .iter()
            .map(|model| model.provider.clone())
            .collect()
    }
}

impl ModelRegistry {
    /// Load models from multiple directories (later directories override earlier ones).
    /// Typical order: user-level (`~/.rozsa/models/`) then project-level (`.rozsa/models/`).
    /// Project-level takes priority because it's loaded last.
    pub fn load_from_dirs(dirs: &[&Path]) -> Result<Self, ModelRegistryError> {
        let mut registry = Self {
            models: Vec::new(),
            user_configured_model_keys: HashSet::new(),
            model_config_paths: HashMap::new(),
            provider_api_keys: HashMap::new(),
        };

        for dir in dirs {
            registry.apply_dir(dir)?;
        }

        Ok(registry)
    }

    /// Load models from a single directory of JSON config files.
    pub fn load_from_dir(dir: &Path) -> Result<Self, ModelRegistryError> {
        Self::load_from_dirs(&[dir])
    }

    /// Scan a directory for `*.json` files and apply each as a models config.
    /// Silently skips if the directory doesn't exist.
    fn apply_dir(&mut self, dir: &Path) -> Result<(), ModelRegistryError> {
        if !dir.is_dir() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(dir)
            .map_err(|e| ModelRegistryError::ModelsJsonRead {
                path: dir.display().to_string(),
                message: e.to_string(),
            })?
            .filter_map(|entry| entry.ok())
            .filter(|entry| is_models_config_path(&entry.path()))
            .collect();

        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            self.apply_models_config_file(&entry.path())?;
        }

        Ok(())
    }

    /// Load model metadata from generated JSON text.
    pub fn from_generated_json(input: &str) -> Result<Self, ModelRegistryError> {
        Ok(Self {
            models: flatten_generated_models(input)?,
            user_configured_model_keys: HashSet::new(),
            model_config_paths: HashMap::new(),
            provider_api_keys: HashMap::new(),
        })
    }

    /// Return all models as runtime Model types.
    pub fn all(&self) -> Vec<rozsa_model::types::Model> {
        self.models.iter().map(|rm| rm.to_model()).collect()
    }

    /// Return all models serializable as JSON for frontend IPC.
    pub fn all_json(&self) -> serde_json::Value {
        serde_json::json!(&self.models)
    }

    /// Find a model by provider and model ID (internal).
    pub fn find(&self, provider: &str, model_id: &str) -> Option<rozsa_model::types::Model> {
        self.models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
            .map(|rm| rm.to_model())
    }

    /// Find a model by provider and ID, returning the runtime Model type.
    pub fn resolve(&self, provider: &str, model_id: &str) -> Option<rozsa_model::types::Model> {
        self.find(provider, model_id)
    }

    /// Find a model by ID only (first match across all providers).
    pub fn find_by_id(&self, model_id: &str) -> Option<rozsa_model::types::Model> {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .map(|rm| rm.to_model())
    }

    /// Return the first available model (one whose provider has an API key configured).
    pub fn first_available(&self) -> Option<rozsa_model::types::Model> {
        let available = self.provider_available();
        self.models
            .iter()
            .find(|m| available.get(&m.provider).is_some_and(|pa| pa.configured))
            .map(|rm| rm.to_model())
    }

    /// Return whether this model was explicitly configured in `models.json`.
    pub fn is_user_configured(&self, provider: &str, model_id: &str) -> bool {
        self.user_configured_model_keys
            .contains(&model_key(provider, model_id))
    }

    /// Return the configuration file that defined or overrode this model.
    pub fn model_config_path(&self, provider: &str, model_id: &str) -> Option<&Path> {
        self.model_config_paths
            .get(&model_key(provider, model_id))
            .map(PathBuf::as_path)
    }

    /// Atomically persist one learned provider-facing thinking effort value.
    pub fn persist_thinking_effort(
        &self,
        provider: &str,
        model_id: &str,
        effort: rozsa_model::types::ThinkingEffort,
        value: Option<&str>,
    ) -> Result<(), ModelRegistryError> {
        static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _write_lock = WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("thinking effort configuration write lock must not be poisoned");
        let path = self.model_config_path(provider, model_id).ok_or_else(|| {
            ModelRegistryError::InvalidModelsJson(format!(
                "No user models configuration owns {provider}/{model_id}"
            ))
        })?;
        let input =
            fs::read_to_string(path).map_err(|error| ModelRegistryError::ModelsJsonRead {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        let mut document: Value = serde_json::from_str(&strip_json_comments(&input))
            .map_err(ModelRegistryError::ModelsJsonParse)?;
        let provider_config = document
            .get_mut("providers")
            .and_then(Value::as_object_mut)
            .and_then(|providers| providers.get_mut(provider))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                ModelRegistryError::InvalidModelsJson(format!("Missing provider {provider}"))
            })?;
        let effort_key = serde_json::to_value(effort)
            .expect("ThinkingEffort serialization is infallible")
            .as_str()
            .expect("ThinkingEffort serializes to a string")
            .to_owned();
        let updated = if let Some(model) = provider_config
            .get_mut("models")
            .and_then(Value::as_array_mut)
            .and_then(|models| {
                models
                    .iter_mut()
                    .find(|model| model.get("id").and_then(Value::as_str) == Some(model_id))
            }) {
            model.as_object_mut()
        } else {
            provider_config
                .get_mut("modelOverrides")
                .and_then(Value::as_object_mut)
                .and_then(|overrides| overrides.get_mut(model_id))
                .and_then(Value::as_object_mut)
        }
        .ok_or_else(|| {
            ModelRegistryError::InvalidModelsJson(format!("Missing model {provider}/{model_id}"))
        })?;
        let map = updated
            .entry("thinkingEffortMap")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                ModelRegistryError::InvalidModelsJson(
                    "thinkingEffortMap must be an object".to_string(),
                )
            })?;
        map.insert(
            effort_key,
            value.map_or(Value::Null, |value| Value::String(value.to_string())),
        );
        let temporary = path.with_extension("json.tmp");
        fs::write(
            &temporary,
            serde_json::to_string_pretty(&document).expect("JSON value serializes"),
        )
        .map_err(|error| ModelRegistryError::ModelsJsonRead {
            path: temporary.display().to_string(),
            message: error.to_string(),
        })?;
        fs::rename(&temporary, path).map_err(|error| ModelRegistryError::ModelsJsonRead {
            path: path.display().to_string(),
            message: error.to_string(),
        })
    }

    /// Merge a `models.json` document into this registry.
    pub fn apply_models_config_json(&mut self, input: &str) -> Result<(), ModelRegistryError> {
        let input = strip_json_comments(input);
        let config: ModelsConfig =
            serde_json::from_str(&input).map_err(ModelRegistryError::ModelsJsonParse)?;
        self.apply_models_config_with_source(config, None)
    }

    /// Merge a `models.json` file into this registry.
    pub fn apply_models_config_file(&mut self, path: &Path) -> Result<(), ModelRegistryError> {
        let input =
            fs::read_to_string(path).map_err(|error| ModelRegistryError::ModelsJsonRead {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        let input = strip_json_comments(&input);
        let config: ModelsConfig =
            serde_json::from_str(&input).map_err(ModelRegistryError::ModelsJsonParse)?;
        self.apply_models_config_with_source(config, Some(path))
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
                thinking_effort_map: None,
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

        for (provider_name, provider_config) in config.providers {
            if let Some(api_key) = &provider_config.api_key {
                self.provider_api_keys
                    .insert(provider_name.clone(), api_key.clone());
            }
            if provider_config.base_url.is_some()
                || provider_config.headers.is_some()
                || provider_config.compat.is_some()
            {
                provider_overrides.insert(
                    provider_name.clone(),
                    ProviderOverride {
                        base_url: provider_config.base_url.clone(),
                        headers: provider_config.headers.clone(),
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

    fn apply_models_config_with_source(
        &mut self,
        config: ModelsConfig,
        source: Option<&Path>,
    ) -> Result<(), ModelRegistryError> {
        let configured_keys = config
            .providers
            .iter()
            .flat_map(|(provider, config)| {
                config
                    .models
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(move |model| model_key(provider, &model.id))
                    .chain(
                        config
                            .model_overrides
                            .as_ref()
                            .into_iter()
                            .flat_map(move |overrides| overrides.keys())
                            .map(move |model_id| model_key(provider, model_id)),
                    )
            })
            .collect::<Vec<_>>();
        self.apply_models_config(config)?;
        if let Some(source) = source {
            for key in configured_keys {
                self.model_config_paths.insert(key, source.to_path_buf());
            }
        }
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
                let uses_external_auth = provider_config.api.as_deref()
                    == Some("bedrock-converse-stream")
                    || provider_name == "amazon-bedrock"
                    || provider_config.auth_header.unwrap_or(false);
                if provider_config.api_key.is_none() && !uses_external_auth {
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
                if let Some(headers) = &override_config.headers {
                    let mut merged = headers.clone();
                    if let Some(model_headers) = model.headers.take() {
                        merged.extend(model_headers);
                    }
                    model.headers = Some(merged);
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

    /// Compute auth availability per provider using env vars and models.json apiKey.
    pub fn provider_available(&self) -> HashMap<String, ProviderAvailable> {
        let mut result = HashMap::new();

        for provider_name in self.provider_ids() {
            // 1. Check models.json apiKey for this provider
            if let Some(api_key) = self.provider_api_keys.get(&provider_name) {
                let source = if api_key.starts_with('!') {
                    "models_json_command"
                } else if std::env::var(api_key).is_ok() {
                    "environment"
                } else {
                    "models_json_key"
                };
                result.insert(
                    provider_name,
                    ProviderAvailable {
                        configured: true,
                        source: Some(source.to_string()),
                    },
                );
                continue;
            }

            // 2. Check known env vars via get_env_api_key
            let provider_enum = provider_from_str(&provider_name);
            if let Some(ref p) = provider_enum {
                if get_env_api_key(p).is_some() {
                    result.insert(
                        provider_name,
                        ProviderAvailable {
                            configured: true,
                            source: Some("environment".to_string()),
                        },
                    );
                    continue;
                }
            }

            // 3. Not configured
            result.insert(
                provider_name,
                ProviderAvailable {
                    configured: false,
                    source: None,
                },
            );
        }

        result
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
    #[serde(rename = "authHeader")]
    auth_header: Option<bool>,
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
    #[serde(rename = "thinkingEffortMap", alias = "thinkingLevelMap")]
    thinking_effort_map: Option<Value>,
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
    #[serde(rename = "thinkingEffortMap", alias = "thinkingLevelMap")]
    thinking_effort_map: Option<Value>,
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
    headers: Option<HashMap<String, String>>,
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

fn flatten_generated_image_models(
    input: &str,
) -> Result<Vec<RegistryImageModel>, ModelRegistryError> {
    let providers: HashMap<String, HashMap<String, RegistryImageModel>> =
        serde_json::from_str(input).map_err(ModelRegistryError::GeneratedImageParse)?;
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
        thinking_effort_map: model_def.thinking_effort_map,
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
    if let Some(thinking_effort_map) = &model_override.thinking_effort_map {
        model.thinking_effort_map = Some(merge_json_objects(
            model.thinking_effort_map.clone(),
            Some(thinking_effort_map.clone()),
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

fn provider_from_str(name: &str) -> Option<Provider> {
    match name {
        "anthropic" => Some(Provider::Anthropic),
        "openai" | "azure-openai-responses" | "codex-oauth" | "github-copilot" => {
            Some(Provider::OpenAI)
        }
        "amazon-bedrock" => Some(Provider::AmazonBedrock),
        "google" => Some(Provider::Google),
        "google-vertex" => Some(Provider::GoogleVertex),
        "deepseek" => Some(Provider::DeepSeek),
        "openrouter" => Some(Provider::OpenRouter),
        "xai" => Some(Provider::XAI),
        "groq" => Some(Provider::Groq),
        "cerebras" => Some(Provider::Cerebras),
        "mistral" => Some(Provider::Mistral),
        "nvidia" => Some(Provider::Nvidia),
        "zai" | "vercel-ai-gateway" => Some(Provider::Zai),
        "together" => Some(Provider::Together),
        "moonshotai" | "kimi-coding" | "opencode" | "opencode-go" => Some(Provider::MoonshotAI),
        "moonshotai-cn" => Some(Provider::MoonshotAICn),
        "huggingface" => Some(Provider::HuggingFace),
        "fireworks" => Some(Provider::MoonshotAI),
        "cloudflare-workers-ai" => Some(Provider::CloudflareWorkersAI),
        "cloudflare-ai-gateway" => Some(Provider::CloudflareAIGateway),
        "xiaomi" => Some(Provider::Xiaomi),
        "xiaomi-token-plan-cn" => Some(Provider::XiaomiTokenPlanCn),
        "xiaomi-token-plan-ams" => Some(Provider::XiaomiTokenPlanAms),
        "xiaomi-token-plan-sgp" => Some(Provider::XiaomiTokenPlanSgp),
        "minimax" | "minimax-cn" => None,
        _ => None,
    }
}

fn is_models_config_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
        && path.file_name().and_then(|name| name.to_str()) != Some("auth.json")
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

#[cfg(test)]
mod tests {
    use super::is_models_config_path;
    use std::path::Path;

    #[test]
    fn models_config_scan_ignores_auth_json() {
        assert!(!is_models_config_path(Path::new("auth.json")));
        assert!(is_models_config_path(Path::new("openai.json")));
        assert!(!is_models_config_path(Path::new("openai.toml")));
    }
}
