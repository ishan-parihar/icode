use std::future::Future;
use std::pin::Pin;

use crate::error::ApiError;
use crate::types::{MessageRequest, MessageResponse};

pub mod anthropic;
pub mod azure;
pub mod bedrock;
pub mod gemini;
pub mod groq;
pub mod mistral;
pub mod openai_compat;
pub mod openrouter;
pub mod registry;
pub use registry::{ProviderRegistry, RegisteredProvider};

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;

pub trait Provider {
    type Stream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse>;

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Xai,
    OpenAi,
    QwenProxy,
    Azure,
    Gemini,
    Bedrock,
    OpenRouter,
    Mistral,
    Groq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

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
    #[expect(clippy::too_many_arguments)]
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
}

pub struct RegistryEntry {
    pub alias: &'static str,
    pub canonical: &'static str,
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
    pub capabilities: ModelCapabilities,
}

const MODEL_REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        alias: "opus",
        canonical: "claude-opus-4-6",
        provider: ProviderKind::Anthropic,
        auth_env: "ANTHROPIC_API_KEY",
        base_url_env: "ANTHROPIC_BASE_URL",
        default_base_url: anthropic::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            200_000, 32_000, true, true, true, 15.0, 75.0, 18.75, 1.50,
        ),
    },
    RegistryEntry {
        alias: "sonnet",
        canonical: "claude-sonnet-4-6",
        provider: ProviderKind::Anthropic,
        auth_env: "ANTHROPIC_API_KEY",
        base_url_env: "ANTHROPIC_BASE_URL",
        default_base_url: anthropic::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            200_000, 64_000, true, true, true, 15.0, 75.0, 18.75, 1.50,
        ),
    },
    RegistryEntry {
        alias: "haiku",
        canonical: "claude-haiku-4-5-20251213",
        provider: ProviderKind::Anthropic,
        auth_env: "ANTHROPIC_API_KEY",
        base_url_env: "ANTHROPIC_BASE_URL",
        default_base_url: anthropic::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            200_000, 8_192, false, true, true, 1.0, 5.0, 1.25, 0.10,
        ),
    },
    RegistryEntry {
        alias: "grok",
        canonical: "grok-3",
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        capabilities: ModelCapabilities::new(131_072, 8_192, true, true, true, 3.0, 15.0, 0.0, 0.0),
    },
    RegistryEntry {
        alias: "grok-3",
        canonical: "grok-3",
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        capabilities: ModelCapabilities::new(131_072, 8_192, true, true, true, 3.0, 15.0, 0.0, 0.0),
    },
    RegistryEntry {
        alias: "grok-mini",
        canonical: "grok-3-mini",
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        capabilities: ModelCapabilities::new(
            131_072, 4_096, true, true, false, 2.0, 10.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "grok-3-mini",
        canonical: "grok-3-mini",
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        capabilities: ModelCapabilities::new(
            131_072, 4_096, true, true, false, 2.0, 10.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "grok-2",
        canonical: "grok-2",
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        capabilities: ModelCapabilities::new(
            131_072, 4_096, false, true, false, 2.0, 10.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "gpt-4o",
        canonical: "gpt-4o",
        provider: ProviderKind::OpenAi,
        auth_env: "OPENAI_API_KEY",
        base_url_env: "OPENAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        capabilities: ModelCapabilities::new(
            128_000, 16_384, true, true, true, 5.0, 15.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "coder-model",
        canonical: "coder-model",
        provider: ProviderKind::QwenProxy,
        auth_env: "QWEN_PROXY_API_KEY",
        base_url_env: "QWEN_PROXY_BASE_URL",
        default_base_url: openai_compat::DEFAULT_QWEN_PROXY_BASE_URL,
        capabilities: ModelCapabilities::new(
            128_000, 8_192, true, true, true, 0.20, 0.60, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "qwen3-coder-plus",
        canonical: "qwen3-coder-plus",
        provider: ProviderKind::QwenProxy,
        auth_env: "QWEN_PROXY_API_KEY",
        base_url_env: "QWEN_PROXY_BASE_URL",
        default_base_url: openai_compat::DEFAULT_QWEN_PROXY_BASE_URL,
        capabilities: ModelCapabilities::new(
            256_000, 12_288, true, true, true, 0.40, 1.20, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "qwen3-coder-flash",
        canonical: "qwen3-coder-flash",
        provider: ProviderKind::QwenProxy,
        auth_env: "QWEN_PROXY_API_KEY",
        base_url_env: "QWEN_PROXY_BASE_URL",
        default_base_url: openai_compat::DEFAULT_QWEN_PROXY_BASE_URL,
        capabilities: ModelCapabilities::new(
            128_000, 4_096, false, true, true, 0.10, 0.30, 0.0, 0.0,
        ),
    },
    // Azure OpenAI
    RegistryEntry {
        alias: "azure/gpt-4",
        canonical: "azure/gpt-4",
        provider: ProviderKind::Azure,
        auth_env: "AZURE_OPENAI_API_KEY",
        base_url_env: "AZURE_OPENAI_RESOURCE",
        default_base_url: "https://.openai.azure.com",
        capabilities: ModelCapabilities::new(
            128_000, 8_192, true, true, true, 10.0, 30.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "azure/gpt-4o",
        canonical: "azure/gpt-4o",
        provider: ProviderKind::Azure,
        auth_env: "AZURE_OPENAI_API_KEY",
        base_url_env: "AZURE_OPENAI_RESOURCE",
        default_base_url: "https://.openai.azure.com",
        capabilities: ModelCapabilities::new(
            128_000, 16_384, true, true, true, 5.0, 15.0, 0.0, 0.0,
        ),
    },
    // Google Gemini
    RegistryEntry {
        alias: "gemini/gemini-2.5-pro",
        canonical: "gemini/gemini-2.5-pro",
        provider: ProviderKind::Gemini,
        auth_env: "GEMINI_API_KEY",
        base_url_env: "GEMINI_BASE_URL",
        default_base_url: gemini::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            1_048_576, 65_536, true, true, true, 1.25, 10.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "gemini/gemini-2.5-flash",
        canonical: "gemini/gemini-2.5-flash",
        provider: ProviderKind::Gemini,
        auth_env: "GEMINI_API_KEY",
        base_url_env: "GEMINI_BASE_URL",
        default_base_url: gemini::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            1_048_576, 65_536, true, true, true, 0.15, 0.60, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "gemini/gemini-2.0-flash",
        canonical: "gemini/gemini-2.0-flash",
        provider: ProviderKind::Gemini,
        auth_env: "GEMINI_API_KEY",
        base_url_env: "GEMINI_BASE_URL",
        default_base_url: gemini::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            1_048_576, 8_192, true, true, true, 0.10, 0.40, 0.0, 0.0,
        ),
    },
    // AWS Bedrock
    RegistryEntry {
        alias: "bedrock/claude-3.5-sonnet",
        canonical: "bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0",
        provider: ProviderKind::Bedrock,
        auth_env: "AWS_ACCESS_KEY_ID",
        base_url_env: "AWS_DEFAULT_REGION",
        default_base_url: "https://bedrock-runtime.us-east-1.amazonaws.com",
        capabilities: ModelCapabilities::new(
            200_000, 8_192, true, true, true, 3.0, 15.0, 3.75, 0.30,
        ),
    },
    RegistryEntry {
        alias: "bedrock/claude-3.5-sonnet-v2",
        canonical: "bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0",
        provider: ProviderKind::Bedrock,
        auth_env: "AWS_ACCESS_KEY_ID",
        base_url_env: "AWS_DEFAULT_REGION",
        default_base_url: "https://bedrock-runtime.us-east-1.amazonaws.com",
        capabilities: ModelCapabilities::new(
            200_000, 8_192, true, true, true, 3.0, 15.0, 3.75, 0.30,
        ),
    },
    RegistryEntry {
        alias: "bedrock/claude-3-opus",
        canonical: "bedrock/anthropic.claude-3-opus-20240229-v1:0",
        provider: ProviderKind::Bedrock,
        auth_env: "AWS_ACCESS_KEY_ID",
        base_url_env: "AWS_DEFAULT_REGION",
        default_base_url: "https://bedrock-runtime.us-east-1.amazonaws.com",
        capabilities: ModelCapabilities::new(
            200_000, 4_096, true, true, true, 15.0, 75.0, 18.75, 1.50,
        ),
    },
    RegistryEntry {
        alias: "bedrock/llama-3.3-70b",
        canonical: "bedrock/meta.llama3-3-70b-instruct-v1:0",
        provider: ProviderKind::Bedrock,
        auth_env: "AWS_ACCESS_KEY_ID",
        base_url_env: "AWS_DEFAULT_REGION",
        default_base_url: "https://bedrock-runtime.us-east-1.amazonaws.com",
        capabilities: ModelCapabilities::new(128_000, 8_192, true, true, false, 2.0, 6.0, 0.0, 0.0),
    },
    // OpenRouter
    RegistryEntry {
        alias: "openrouter/claude-sonnet",
        canonical: "openrouter/anthropic/claude-3.5-sonnet",
        provider: ProviderKind::OpenRouter,
        auth_env: "OPENROUTER_API_KEY",
        base_url_env: "OPENROUTER_BASE_URL",
        default_base_url: openrouter::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(200_000, 8_192, true, true, true, 3.0, 15.0, 0.0, 0.0),
    },
    RegistryEntry {
        alias: "openrouter/claude-opus",
        canonical: "openrouter/anthropic/claude-opus",
        provider: ProviderKind::OpenRouter,
        auth_env: "OPENROUTER_API_KEY",
        base_url_env: "OPENROUTER_BASE_URL",
        default_base_url: openrouter::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            200_000, 16_384, true, true, true, 15.0, 75.0, 0.0, 0.0,
        ),
    },
    // Mistral
    RegistryEntry {
        alias: "mistral/mistral-large",
        canonical: "mistral/mistral-large-latest",
        provider: ProviderKind::Mistral,
        auth_env: "MISTRAL_API_KEY",
        base_url_env: "MISTRAL_BASE_URL",
        default_base_url: mistral::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(128_000, 8_192, true, true, false, 2.0, 6.0, 0.0, 0.0),
    },
    RegistryEntry {
        alias: "mistral/mistral-small",
        canonical: "mistral/mistral-small-latest",
        provider: ProviderKind::Mistral,
        auth_env: "MISTRAL_API_KEY",
        base_url_env: "MISTRAL_BASE_URL",
        default_base_url: mistral::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            32_000, 8_192, true, true, false, 0.20, 0.60, 0.0, 0.0,
        ),
    },
    // Groq
    RegistryEntry {
        alias: "groq/llama-3.3-70b",
        canonical: "groq/llama-3.3-70b-versatile",
        provider: ProviderKind::Groq,
        auth_env: "GROQ_API_KEY",
        base_url_env: "GROQ_BASE_URL",
        default_base_url: groq::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            128_000, 32_768, true, true, false, 0.0, 0.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "groq/llama-3.1-8b",
        canonical: "groq/llama-3.1-8b-instant",
        provider: ProviderKind::Groq,
        auth_env: "GROQ_API_KEY",
        base_url_env: "GROQ_BASE_URL",
        default_base_url: groq::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            128_000, 8_192, false, true, false, 0.0, 0.0, 0.0, 0.0,
        ),
    },
    RegistryEntry {
        alias: "groq/mixtral-8x7b",
        canonical: "groq/mixtral-8x7b-32768",
        provider: ProviderKind::Groq,
        auth_env: "GROQ_API_KEY",
        base_url_env: "GROQ_BASE_URL",
        default_base_url: groq::DEFAULT_BASE_URL,
        capabilities: ModelCapabilities::new(
            32_768, 32_768, false, true, false, 0.0, 0.0, 0.0, 0.0,
        ),
    },
];

#[must_use]
pub fn resolve_model_alias(model: &str) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();
    MODEL_REGISTRY
        .iter()
        .find_map(|entry| (*entry.alias == lower).then_some(entry.canonical))
        .map_or_else(|| trimmed.to_string(), ToOwned::to_owned)
}

#[must_use]
pub fn metadata_for_model(model: &str) -> Option<ProviderMetadata> {
    let canonical = resolve_model_alias(model);
    let entry = MODEL_REGISTRY
        .iter()
        .find(|e| e.canonical == canonical || e.alias == model)?;
    Some(ProviderMetadata {
        provider: entry.provider,
        auth_env: entry.auth_env,
        base_url_env: entry.base_url_env,
        default_base_url: entry.default_base_url,
    })
}

#[must_use]
pub fn detect_provider_kind(model: &str) -> ProviderKind {
    if let Some(metadata) = metadata_for_model(model) {
        return metadata.provider;
    }
    let lower = model.to_lowercase();
    if lower.starts_with("azure/") {
        return ProviderKind::Azure;
    }
    if lower.starts_with("gemini/") {
        return ProviderKind::Gemini;
    }
    if lower.starts_with("bedrock/") {
        return ProviderKind::Bedrock;
    }
    if lower.starts_with("openrouter/") {
        return ProviderKind::OpenRouter;
    }
    if lower.starts_with("mistral/") {
        return ProviderKind::Mistral;
    }
    if lower.starts_with("groq/") {
        return ProviderKind::Groq;
    }
    if anthropic::has_auth_from_env_or_saved().unwrap_or(false) {
        return ProviderKind::Anthropic;
    }
    if openai_compat::has_api_key("OPENAI_API_KEY") {
        return ProviderKind::OpenAi;
    }
    if openai_compat::has_api_key("XAI_API_KEY") {
        return ProviderKind::Xai;
    }
    if openai_compat::has_api_key("QWEN_PROXY_API_KEY")
        || std::env::var("QWEN_PROXY_BASE_URL").is_ok()
    {
        return ProviderKind::QwenProxy;
    }
    ProviderKind::Anthropic
}

#[must_use]
pub fn capabilities_for_model(model: &str) -> ModelCapabilities {
    let canonical = resolve_model_alias(model);
    MODEL_REGISTRY
        .iter()
        .find(|e| e.canonical == canonical || e.alias == model)
        .map_or_else(
            || {
                if canonical.starts_with("claude") {
                    ModelCapabilities::new(
                        200_000, 64_000, true, true, true, 15.0, 75.0, 18.75, 1.50,
                    )
                } else if canonical.starts_with("grok") {
                    ModelCapabilities::new(131_072, 8_192, true, true, false, 3.0, 15.0, 0.0, 0.0)
                } else if canonical.starts_with("gpt") {
                    ModelCapabilities::new(128_000, 16_384, true, true, true, 5.0, 15.0, 0.0, 0.0)
                } else if canonical.starts_with("azure/") {
                    ModelCapabilities::new(128_000, 8_192, true, true, true, 5.0, 15.0, 0.0, 0.0)
                } else if canonical.starts_with("gemini/") {
                    ModelCapabilities::new(
                        1_048_576, 65_536, true, true, true, 1.25, 10.0, 0.0, 0.0,
                    )
                } else if canonical.starts_with("bedrock/") {
                    ModelCapabilities::new(200_000, 8_192, true, true, true, 3.0, 15.0, 3.75, 0.30)
                } else if canonical.starts_with("openrouter/") {
                    ModelCapabilities::new(128_000, 8_192, true, true, true, 3.0, 15.0, 0.0, 0.0)
                } else if canonical.starts_with("mistral/") {
                    ModelCapabilities::new(128_000, 8_192, true, true, false, 2.0, 6.0, 0.0, 0.0)
                } else if canonical.starts_with("groq/") {
                    ModelCapabilities::new(128_000, 32_768, true, true, false, 0.0, 0.0, 0.0, 0.0)
                } else {
                    ModelCapabilities::new(128_000, 8_192, false, true, false, 0.20, 0.60, 0.0, 0.0)
                }
            },
            |e| e.capabilities,
        )
}

#[must_use]
pub fn max_tokens_for_model(model: &str) -> u32 {
    capabilities_for_model(model).max_output
}

pub fn list_all_models() -> impl Iterator<Item = &'static RegistryEntry> {
    MODEL_REGISTRY.iter()
}

#[cfg(test)]
mod tests {
    use super::{
        capabilities_for_model, detect_provider_kind, max_tokens_for_model, resolve_model_alias,
        ProviderKind,
    };

    #[test]
    fn resolves_grok_aliases() {
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
        assert_eq!(resolve_model_alias("grok-2"), "grok-2");
    }

    #[test]
    fn detects_provider_from_model_name_first() {
        assert_eq!(detect_provider_kind("grok"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::Anthropic
        );
    }

    #[test]
    fn resolves_openai_alias() {
        assert_eq!(resolve_model_alias("gpt-4o"), "gpt-4o");
        assert_eq!(detect_provider_kind("gpt-4o"), ProviderKind::OpenAi);
    }

    #[test]
    fn capabilities_match_expected_values() {
        let opus = capabilities_for_model("opus");
        assert_eq!(opus.context_window, 200_000);
        assert_eq!(opus.max_output, 32_000);
        assert!(opus.supports_reasoning);
        assert!(opus.supports_tools);
        assert!(opus.supports_images);

        let haiku = capabilities_for_model("haiku");
        assert!(!haiku.supports_reasoning);
        assert_eq!(haiku.context_window, 200_000);
        assert_eq!(haiku.max_output, 8_192);

        let grok = capabilities_for_model("grok-3");
        assert_eq!(grok.context_window, 131_072);
        assert!(grok.supports_reasoning);

        let unknown = capabilities_for_model("some-unknown-model");
        assert_eq!(unknown.context_window, 128_000);
    }

    #[test]
    fn max_tokens_uses_capabilities() {
        assert_eq!(max_tokens_for_model("opus"), 32_000);
        assert_eq!(max_tokens_for_model("haiku"), 8_192);
        assert_eq!(max_tokens_for_model("grok-3"), 8_192);
        assert_eq!(max_tokens_for_model("gpt-4o"), 16_384);
    }
}
