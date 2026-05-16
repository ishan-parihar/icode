/// Provider routing module.
///
/// All model/provider knowledge comes from the live models.dev catalog
/// (crate::models_dev). There is no static MODEL_REGISTRY anymore.
///
/// Routing logic (which wire protocol to use) maps provider IDs to clients:
/// - "anthropic"           → AnthropicClient  (Anthropic Messages API)
/// - "google" / "gemini"   → GeminiClient
/// - "amazon-bedrock"      → BedrockClient
/// - "azure"               → AzureClient
/// - everything else       → OpenAiCompatClient  (OpenAI-compatible)
///
/// This matches opencode's BUNDLED_PROVIDERS pattern where every non-Anthropic
/// provider is reached via the OpenAI-compatible chat/completions endpoint.
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use runtime::{AuthStore, ProviderConfig};

use crate::error::ApiError;
use crate::models_dev::{self, ModelEntry};
use crate::types::{MessageRequest, MessageResponse};

pub mod anthropic;
pub mod azure;
pub mod bedrock;
pub mod gemini;
pub mod openai_compat;

// These are still compiled (client.rs uses them for routing) but the model
// *lists* inside them are gone — the catalog is the source of truth now.
pub mod groq;
pub mod mistral;
pub mod openrouter;
pub mod registry;

pub use registry::{ProviderRegistry, RegisteredProvider};

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;

pub trait Provider {
    type Stream;
    fn send_message<'a>(&'a self, req: &'a MessageRequest) -> ProviderFuture<'a, MessageResponse>;
    fn stream_message<'a>(&'a self, req: &'a MessageRequest) -> ProviderFuture<'a, Self::Stream>;
}

// ── ProviderKind ──────────────────────────────────────────────────────────────

/// Wire-protocol discriminant. Only providers with distinct HTTP protocols
/// have their own variant. Everything OpenAI-compatible shares the same client.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    // Distinct-protocol providers
    Anthropic,
    Gemini,
    Bedrock,
    Azure,
    // OpenAI-compatible providers (all share OpenAiCompatClient)
    OpenAi,
    Xai,
    Groq,
    Mistral,
    OpenRouter,
    Opencode,
    QwenProxy,
    /// Any catalog provider not listed above
    CustomOpenAi { provider: String, model: String },
    Unconfigured,
}

impl ProviderKind {
    /// Return the catalog provider ID for this kind.
    pub fn catalog_id(&self) -> &str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Gemini => "google",
            Self::Bedrock => "amazon-bedrock",
            Self::Azure => "azure",
            Self::OpenAi => "openai",
            Self::Xai => "xai",
            Self::Groq => "groq",
            Self::Mistral => "mistral",
            Self::OpenRouter => "openrouter",
            Self::Opencode => "opencode",
            Self::QwenProxy => "qwen",
            Self::CustomOpenAi { provider, .. } => provider.as_str(),
            Self::Unconfigured => "",
        }
    }

    /// True if this kind uses the OpenAI-compatible wire protocol.
    pub fn is_openai_compat(&self) -> bool {
        !matches!(self, Self::Anthropic | Self::Gemini | Self::Bedrock | Self::Azure | Self::Unconfigured)
    }
}

// ── ModelCapabilities ─────────────────────────────────────────────────────────

/// Capability snapshot for a model — derived from the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModelCapabilities {
    pub context_window: u32,
    pub max_output: u32,
    pub supports_reasoning: bool,
    pub supports_tools: bool,
    pub supports_images: bool,
    pub cost_input_per_million: f64,
    pub cost_output_per_million: f64,
    pub cost_cache_create_per_million: f64,
    pub cost_cache_read_per_million: f64,
}

impl ModelCapabilities {
    #[must_use]
    pub const fn new(
        context_window: u32,
        max_output: u32,
        supports_reasoning: bool,
        supports_tools: bool,
        supports_images: bool,
        cost_input: f64,
        cost_output: f64,
        cost_cache_create: f64,
        cost_cache_read: f64,
    ) -> Self {
        Self {
            context_window,
            max_output,
            supports_reasoning,
            supports_tools,
            supports_images,
            cost_input_per_million: cost_input,
            cost_output_per_million: cost_output,
            cost_cache_create_per_million: cost_cache_create,
            cost_cache_read_per_million: cost_cache_read,
        }
    }

    fn from_entry(e: &ModelEntry) -> Self {
        Self {
            context_window: e.context_window,
            max_output: e.max_output,
            supports_reasoning: e.supports_reasoning,
            supports_tools: e.supports_tools,
            supports_images: e.supports_images,
            cost_input_per_million: e.cost_input,
            cost_output_per_million: e.cost_output,
            ..Default::default()
        }
    }
}

// ── RegistryEntry shim ────────────────────────────────────────────────────────

/// Thin wrapper that presents catalog data in the shape that model_picker.rs,
/// dialog_providers.rs, and other TUI code expect.
pub struct RegistryEntry {
    pub alias: String,
    pub canonical: String,
    pub provider: ProviderKind,
    pub capabilities: ModelCapabilities,
}

impl RegistryEntry {
    fn from_catalog(e: &ModelEntry) -> Self {
        Self {
            alias: e.model_id.clone(),
            canonical: e.model_id.clone(),
            provider: provider_kind_for_id(&e.provider_id),
            capabilities: ModelCapabilities::from_entry(e),
        }
    }
}

// ── Provider-kind detection ───────────────────────────────────────────────────

/// Map a catalog provider ID to its wire-protocol kind.
pub fn provider_kind_for_id(provider_id: &str) -> ProviderKind {
    match provider_id {
        "anthropic" => ProviderKind::Anthropic,
        "google" | "gemini" | "google-vertex" => ProviderKind::Gemini,
        "amazon-bedrock" => ProviderKind::Bedrock,
        "azure" | "azure-cognitive-services" => ProviderKind::Azure,
        "openai" => ProviderKind::OpenAi,
        "xai" => ProviderKind::Xai,
        "groq" => ProviderKind::Groq,
        "mistral" => ProviderKind::Mistral,
        "openrouter" => ProviderKind::OpenRouter,
        "opencode" => ProviderKind::Opencode,
        "qwen" | "qwen-proxy" => ProviderKind::QwenProxy,
        other => ProviderKind::CustomOpenAi {
            provider: other.to_string(),
            model: String::new(),
        },
    }
}

/// Given a model string `"provider_id/model_id"`, detect the wire-protocol kind.
/// Falls back to checking the live catalog for unknown prefixes.
#[must_use]
pub fn detect_provider_kind(
    model: &str,
    providers: Option<&BTreeMap<String, ProviderConfig>>,
) -> ProviderKind {
    // Config-driven custom providers take precedence.
    if let Some((prefix, rest)) = model.split_once('/') {
        if let Some(providers_map) = providers {
            if providers_map.contains_key(prefix) {
                return ProviderKind::CustomOpenAi {
                    provider: prefix.to_string(),
                    model: rest.to_string(),
                };
            }
        }
    }

    // Well-known prefixes — handle without catalog (works offline too).
    let lower = model.to_lowercase();
    if lower.starts_with("anthropic/") || lower.starts_with("claude") {
        return ProviderKind::Anthropic;
    }
    if lower.starts_with("gemini/") || lower.starts_with("google/") {
        return ProviderKind::Gemini;
    }
    if lower.starts_with("bedrock/") || lower.starts_with("amazon-bedrock/") {
        return ProviderKind::Bedrock;
    }
    if lower.starts_with("azure/") {
        return ProviderKind::Azure;
    }
    if lower.starts_with("openai/") || lower.starts_with("gpt") {
        return ProviderKind::OpenAi;
    }
    if lower.starts_with("xai/") || lower.starts_with("grok") {
        return ProviderKind::Xai;
    }
    if lower.starts_with("groq/") {
        return ProviderKind::Groq;
    }
    if lower.starts_with("mistral/") {
        return ProviderKind::Mistral;
    }
    if lower.starts_with("openrouter/") {
        return ProviderKind::OpenRouter;
    }
    if lower.starts_with("opencode/") {
        return ProviderKind::Opencode;
    }

    // Look up in the live catalog by "provider_id/model_id".
    if let Some((provider_id, _)) = model.split_once('/') {
        let cat = models_dev::catalog();
        if cat.contains_key(provider_id) {
            return provider_kind_for_id(provider_id);
        }
        return ProviderKind::Unconfigured;
    }

    // Plain model name (no slash) — check if catalog has an exact match.
    {
        let cat = models_dev::catalog();
        for (pid, p) in &cat {
            if p.models.contains_key(model) {
                return provider_kind_for_id(pid);
            }
        }
    }

    ProviderKind::Unconfigured
}

/// Resolve short aliases (e.g. "sonnet" → "anthropic/claude-sonnet-4-5").
/// With catalog-driven lookup we don't maintain a static alias table;
/// the model ID is passed through unchanged unless it matches a catalog model name directly.
#[must_use]
pub fn resolve_model_alias(model: &str) -> String {
    model.trim().to_string()
}

// ── Model listing ─────────────────────────────────────────────────────────────

/// Return all active models from the live catalog.
pub fn list_all_models() -> impl Iterator<Item = RegistryEntry> {
    models_dev::list_models()
        .into_iter()
        .map(|e| RegistryEntry::from_catalog(&e))
}

/// Look up capabilities for a model from the catalog.
#[must_use]
pub fn capabilities_for_model(model: &str) -> ModelCapabilities {
    // Strip provider prefix to find the model in the catalog.
    let model_id = model.split_once('/').map(|(_, m)| m).unwrap_or(model);
    let cat = models_dev::catalog();
    for p in cat.values() {
        if let Some(m) = p.models.get(model_id).or_else(|| p.models.get(model)) {
            return ModelCapabilities {
                context_window: m.limit.context,
                max_output: m.limit.output,
                supports_reasoning: m.reasoning,
                supports_tools: m.tool_call,
                supports_images: m
                    .modalities
                    .as_ref()
                    .map(|mo| mo.input.iter().any(|s| s == "image"))
                    .unwrap_or(false),
                cost_input_per_million: m.cost.as_ref().map(|c| c.input).unwrap_or(0.0),
                cost_output_per_million: m.cost.as_ref().map(|c| c.output).unwrap_or(0.0),
                ..Default::default()
            };
        }
    }
    // Unknown model — return a reasonable default.
    ModelCapabilities::new(128_000, 8_192, false, true, false, 0.0, 0.0, 0.0, 0.0)
}

#[must_use]
pub fn max_tokens_for_model(model: &str) -> u32 {
    capabilities_for_model(model).max_output
}

// ── Auth checking ─────────────────────────────────────────────────────────────

/// Check whether a given provider kind has authentication available.
#[must_use]
pub fn check_provider_auth(kind: &ProviderKind) -> bool {
    match kind {
        ProviderKind::Anthropic => {
            anthropic::has_auth_from_env_or_saved().unwrap_or(false)
        }
        ProviderKind::Bedrock => {
            ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_PROFILE", "AWS_BEARER_TOKEN_BEDROCK"]
                .iter()
                .any(|v| std::env::var(v).is_ok())
        }
        ProviderKind::Gemini => openai_compat::has_api_key("GEMINI_API_KEY"),
        ProviderKind::Azure => openai_compat::has_api_key("AZURE_OPENAI_API_KEY"),
        ProviderKind::OpenAi => openai_compat::has_api_key("OPENAI_API_KEY"),
        ProviderKind::Xai => openai_compat::has_api_key("XAI_API_KEY"),
        ProviderKind::Groq => openai_compat::has_api_key("GROQ_API_KEY"),
        ProviderKind::Mistral => openai_compat::has_api_key("MISTRAL_API_KEY"),
        ProviderKind::OpenRouter => openai_compat::has_api_key("OPENROUTER_API_KEY"),
        ProviderKind::Opencode => openai_compat::has_api_key("OPENCODE_API_KEY"),
        ProviderKind::QwenProxy => {
            openai_compat::has_api_key("QWEN_PROXY_API_KEY")
                || std::env::var("QWEN_PROXY_BASE_URL").is_ok()
        }
        ProviderKind::CustomOpenAi { provider, .. } => {
            let cat = models_dev::catalog();
            if let Some(p) = cat.get(provider.as_str()) {
                return models_dev::provider_has_auth(p);
            }
            let env_key = format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"));
            if std::env::var(&env_key).map(|v| !v.is_empty()).unwrap_or(false) {
                return true;
            }
            let store = AuthStore::load();
            store.api_key_for(&provider.to_lowercase().replace('-', "_")).is_some()
        }
        ProviderKind::Unconfigured => false,
    }
}

#[must_use]
pub fn is_provider_configured(kind: &ProviderKind) -> bool {
    check_provider_auth(kind)
}

// ── Provider auth status ──────────────────────────────────────────────────────

/// Auth status for one provider, used by the TUI Providers dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAuthStatus {
    pub kind: ProviderKind,
    pub display_name: Cow<'static, str>,
    pub env_vars: Vec<Cow<'static, str>>,
    pub has_auth: bool,
    pub model_count: usize,
}

/// Scan all catalog providers and report auth status.
#[must_use]
pub fn scan_provider_auth_status(
    extra_providers: Option<&BTreeMap<String, ProviderConfig>>,
) -> Vec<ProviderAuthStatus> {
    let cat = models_dev::catalog();
    let mut seen = HashSet::new();
    let mut result: Vec<ProviderAuthStatus> = cat
        .values()
        .map(|p| {
            let kind = provider_kind_for_id(&p.id);
            let has_auth = models_dev::provider_has_auth(p);
            ProviderAuthStatus {
                kind,
                display_name: Cow::Owned(p.name.clone()),
                env_vars: p.env.iter().map(|v| Cow::Owned(v.clone())).collect(),
                has_auth,
                model_count: p.models.len(),
            }
        })
        .filter(|s| {
            // Deduplicate: azure/azure-cognitive-services both map to Azure kind
            seen.insert(s.display_name.clone())
        })
        .collect();

    result.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    if let Some(providers_map) = extra_providers {
        for (name, config) in providers_map {
            let env_key = &config.api_key_env;
            let has_auth = std::env::var(env_key).map(|v| !v.is_empty()).unwrap_or(false);
            result.push(ProviderAuthStatus {
                kind: ProviderKind::CustomOpenAi { provider: name.clone(), model: String::new() },
                display_name: Cow::Owned(name.clone()),
                env_vars: vec![Cow::Owned(config.api_key_env.clone())],
                has_auth,
                model_count: 0,
            });
        }
    }

    result
}

/// Display name for a provider kind.
#[must_use]
pub fn provider_display_name(kind: &ProviderKind) -> Cow<'_, str> {
    match kind {
        ProviderKind::Anthropic => Cow::Borrowed("Anthropic"),
        ProviderKind::Gemini => Cow::Borrowed("Google Gemini"),
        ProviderKind::Bedrock => Cow::Borrowed("AWS Bedrock"),
        ProviderKind::Azure => Cow::Borrowed("Azure"),
        ProviderKind::OpenAi => Cow::Borrowed("OpenAI"),
        ProviderKind::Xai => Cow::Borrowed("xAI"),
        ProviderKind::Groq => Cow::Borrowed("Groq"),
        ProviderKind::Mistral => Cow::Borrowed("Mistral"),
        ProviderKind::OpenRouter => Cow::Borrowed("OpenRouter"),
        ProviderKind::Opencode => Cow::Borrowed("OpenCode Zen"),
        ProviderKind::QwenProxy => Cow::Borrowed("Qwen Proxy"),
        ProviderKind::CustomOpenAi { provider, .. } => {
            let cat = models_dev::catalog();
            Cow::Owned(
                cat.get(provider.as_str())
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| provider.clone()),
            )
        }
        ProviderKind::Unconfigured => Cow::Borrowed("Unconfigured"),
    }
}

/// Placeholder — the catalog is the PROVIDER_DISPLAY_NAMES source now.
/// We keep this as an empty slice to not break imports.
pub const PROVIDER_DISPLAY_NAMES: &[(ProviderKind, &str)] = &[];

// ── Legacy compat ─────────────────────────────────────────────────────────────

/// `ProviderMetadata` was returned by the old static registry.
/// Kept for call-sites that haven't migrated yet; filled from the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: String,
    pub base_url_env: String,
    pub default_base_url: String,
}

pub fn metadata_for_model(model: &str) -> Option<ProviderMetadata> {
    let (provider_id, model_id) = model.split_once('/')?;
    let cat = models_dev::catalog();
    let p = cat.get(provider_id)?;
    let _ = p.models.get(model_id)?; // confirm model exists
    let env = p.env.first().cloned().unwrap_or_default();
    Some(ProviderMetadata {
        provider: provider_kind_for_id(provider_id),
        auth_env: env.clone(),
        base_url_env: format!("{}_BASE_URL", provider_id.to_uppercase().replace('-', "_")),
        default_base_url: p.api.clone().unwrap_or_default(),
    })
}
