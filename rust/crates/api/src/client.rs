use crate::error::ApiError;
use crate::prompt_cache::{PromptCache, PromptCacheRecord, PromptCacheStats};
use crate::providers::anthropic::{self, AnthropicClient, AuthSource};
use crate::providers::azure::AzureClient;
use crate::providers::bedrock::BedrockClient;
use crate::providers::gemini::GeminiClient;
use crate::providers::groq::GroqClient;
use crate::providers::mistral::MistralClient;
use crate::providers::openai_compat::{self, OpenAiCompatClient, OpenAiCompatConfig};
use crate::providers::openrouter::OpenRouterClient;
use crate::providers::{self, ProviderKind};
use crate::types::{MessageRequest, MessageResponse, StreamEvent};
use runtime::ProviderConfig;
use std::collections::BTreeMap;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ProviderClient {
    Anthropic(AnthropicClient),
    Xai(OpenAiCompatClient),
    OpenAi(OpenAiCompatClient),
    QwenProxy(OpenAiCompatClient),
    Azure(AzureClient),
    Gemini(GeminiClient),
    Bedrock(BedrockClient),
    OpenRouter(OpenRouterClient),
    Mistral(MistralClient),
    Groq(GroqClient),
    /// `OpenCode` Zen provider.
    Opencode(OpenAiCompatClient),
    /// Custom OpenAI-compatible provider configured via settings.json.
    CustomOpenAi(OpenAiCompatClient),
    Unconfigured {
        model: String,
    },
}

#[derive(Debug)]
pub enum MessageStream {
    Anthropic(anthropic::MessageStream),
    OpenAiCompat(openai_compat::MessageStream),
    Azure(crate::providers::azure::MessageStream),
    Gemini(crate::providers::gemini::MessageStream),
    Bedrock(crate::providers::bedrock::MessageStream),
    OpenRouter(crate::providers::openrouter::MessageStream),
    Mistral(crate::providers::mistral::MessageStream),
    Groq(crate::providers::groq::MessageStream),
}

impl ProviderClient {
    pub fn from_model(model: &str) -> Result<Self, ApiError> {
        Self::from_model_with_providers(model, None, None)
    }

    pub fn from_model_with_anthropic_auth(
        model: &str,
        anthropic_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        Self::from_model_with_providers(model, None, anthropic_auth)
    }

    pub fn from_model_with_providers(
        model: &str,
        providers: Option<&BTreeMap<String, ProviderConfig>>,
        anthropic_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        let resolved_model = providers::resolve_model_alias(model);
        let result = match providers::detect_provider_kind(&resolved_model, providers) {
            ProviderKind::Anthropic => match anthropic_auth {
                Some(auth) => Ok(Self::Anthropic(AnthropicClient::from_auth(auth))),
                None => AnthropicClient::from_env().map(Self::Anthropic),
            },
            ProviderKind::Xai => {
                OpenAiCompatClient::from_env(OpenAiCompatConfig::xai()).map(Self::Xai)
            }
            ProviderKind::OpenAi => {
                OpenAiCompatClient::from_env(OpenAiCompatConfig::openai()).map(Self::OpenAi)
            }
            ProviderKind::QwenProxy => {
                let config = OpenAiCompatConfig::qwen_proxy();
                // QwenProxy uses an OpenAI-compatible local proxy that doesn't validate API keys.
                // Use env var if set, otherwise fall back to "none" (matches opencode config).
                let api_key =
                    std::env::var("QWEN_PROXY_API_KEY").unwrap_or_else(|_| "none".to_string());
                Ok(Self::QwenProxy(
                    OpenAiCompatClient::new(api_key, config)
                        .with_base_url(read_qwen_proxy_base_url()),
                ))
            }
            ProviderKind::Azure => AzureClient::from_env().map(Self::Azure),
            ProviderKind::Gemini => GeminiClient::from_env().map(Self::Gemini),
            ProviderKind::Bedrock => BedrockClient::from_env().map(Self::Bedrock),
            ProviderKind::OpenRouter => OpenRouterClient::from_env().map(Self::OpenRouter),
            ProviderKind::Mistral => MistralClient::from_env().map(Self::Mistral),
            ProviderKind::Groq => GroqClient::from_env().map(Self::Groq),
            ProviderKind::Opencode => {
                OpenAiCompatClient::from_env(OpenAiCompatConfig::opencode()).map(Self::Opencode)
            }
            ProviderKind::CustomOpenAi {
                provider,
                model: model_name,
            } => Self::from_custom_openai(&provider, &model_name, providers),
            ProviderKind::Unconfigured => Ok(Self::Unconfigured {
                model: resolved_model.clone(),
            }),
        };
        // If credentials are missing, return Unconfigured instead of erroring.
        // This allows the TUI to start and lets users configure providers interactively.
        // The actual error is deferred until an API call is made.
        match result {
            Err(ApiError::MissingCredentials { .. } | ApiError::Auth(_)) => {
                Ok(Self::Unconfigured {
                    model: resolved_model.clone(),
                })
            }
            other => other,
        }
    }

    fn from_custom_openai(
        provider: &str,
        model_name: &str,
        providers: Option<&BTreeMap<String, ProviderConfig>>,
    ) -> Result<Self, ApiError> {
        match providers.and_then(|p| p.get(provider)) {
            Some(pc) => {
                OpenAiCompatClient::custom_from_config(provider, pc).map(Self::CustomOpenAi)
            }
            None => OpenAiCompatClient::custom(provider, model_name).map(Self::CustomOpenAi),
        }
    }

    #[must_use]
    pub fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::Anthropic(_) => ProviderKind::Anthropic,
            Self::Xai(_) => ProviderKind::Xai,
            Self::OpenAi(_) => ProviderKind::OpenAi,
            Self::QwenProxy(_) => ProviderKind::QwenProxy,
            Self::Azure(_) => ProviderKind::Azure,
            Self::Gemini(_) => ProviderKind::Gemini,
            Self::Bedrock(_) => ProviderKind::Bedrock,
            Self::OpenRouter(_) => ProviderKind::OpenRouter,
            Self::Mistral(_) => ProviderKind::Mistral,
            Self::Groq(_) => ProviderKind::Groq,
            Self::Opencode(_) => ProviderKind::Opencode,
            // CustomOpenAi is a configured provider, not unconfigured.
            // Return a synthetic ProviderKind to distinguish from Unconfigured.
            Self::CustomOpenAi(_) => ProviderKind::CustomOpenAi {
                provider: String::new(),
                model: String::new(),
            },
            Self::Unconfigured { .. } => ProviderKind::Unconfigured,
        }
    }

    #[must_use]
    pub fn with_prompt_cache(self, prompt_cache: PromptCache) -> Self {
        match self {
            Self::Anthropic(client) => Self::Anthropic(client.with_prompt_cache(prompt_cache)),
            other => other,
        }
    }

    #[must_use]
    pub fn prompt_cache_stats(&self) -> Option<PromptCacheStats> {
        match self {
            Self::Anthropic(client) => client.prompt_cache_stats(),
            _ => None,
        }
    }

    #[must_use]
    pub fn take_last_prompt_cache_record(&self) -> Option<PromptCacheRecord> {
        match self {
            Self::Anthropic(client) => client.take_last_prompt_cache_record(),
            _ => None,
        }
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        match self {
            Self::Anthropic(client) => client.send_message(request).await,
            Self::Xai(client)
            | Self::OpenAi(client)
            | Self::QwenProxy(client)
            | Self::Opencode(client)
            | Self::CustomOpenAi(client) => client.send_message(request).await,
            Self::Azure(client) => client.send_message(request).await,
            Self::Gemini(client) => client.send_message(request).await,
            Self::Bedrock(client) => client.send_message(request).await,
            Self::OpenRouter(client) => client.send_message(request).await,
            Self::Mistral(client) => client.send_message(request).await,
            Self::Groq(client) => client.send_message(request).await,
            Self::Unconfigured { model } => {
                Err(ApiError::Auth(unconfigured_auth_message(model)))
            }
        }
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        match self {
            Self::Anthropic(client) => client
                .stream_message(request)
                .await
                .map(MessageStream::Anthropic),
            Self::Xai(client)
            | Self::OpenAi(client)
            | Self::QwenProxy(client)
            | Self::Opencode(client)
            | Self::CustomOpenAi(client) => client
                .stream_message(request)
                .await
                .map(MessageStream::OpenAiCompat),
            Self::Azure(client) => {
                let s: crate::providers::azure::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::Azure(s))
            }
            Self::Gemini(client) => {
                let s: crate::providers::gemini::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::Gemini(s))
            }
            Self::Bedrock(client) => {
                let s: crate::providers::bedrock::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::Bedrock(s))
            }
            Self::OpenRouter(client) => {
                let s: crate::providers::openrouter::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::OpenRouter(s))
            }
            Self::Mistral(client) => {
                let s: crate::providers::mistral::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::Mistral(s))
            }
            Self::Groq(client) => {
                let s: crate::providers::groq::MessageStream =
                    client.stream_message(request).await?;
                Ok(MessageStream::Groq(s))
            }
            Self::Unconfigured { model } => {
                Err(ApiError::Auth(unconfigured_auth_message(model)))
            }
        }
    }
}

/// Produce a provider-specific authentication error message for an unconfigured model.
fn unconfigured_auth_message(model: &str) -> String {
    // First try registry lookup for a precise match.
    if let Some(metadata) = providers::metadata_for_model(model) {
        let env = metadata.auth_env;
        return format!(
            "Model '{model}' requires credentials. \
             Set the {env} environment variable or use `icode login` to configure the provider."
        );
    }

    let model_lower = model.to_lowercase();
    if model_lower.starts_with("claude") {
        format!(
            "Model '{model}' requires Anthropic credentials. \
             Set ANTHROPIC_API_KEY or run `icode login` for OAuth."
        )
    } else if model_lower.starts_with("gpt") || model_lower.starts_with("o1") || model_lower.starts_with("o3") {
        format!(
            "Model '{model}' requires OpenAI credentials. \
             Set OPENAI_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("grok") {
        format!(
            "Model '{model}' requires xAI credentials. \
             Set XAI_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("qwen") {
        format!(
            "Model '{model}' uses the Qwen Proxy provider. \
             Ensure the proxy is running and set QWEN_PROXY_BASE_URL."
        )
    } else if model_lower.starts_with("gemini/") {
        format!(
            "Model '{model}' requires Google Gemini credentials. \
             Set GEMINI_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("azure/") {
        format!(
            "Model '{model}' requires Azure OpenAI credentials. \
             Set AZURE_OPENAI_API_KEY and AZURE_OPENAI_RESOURCE."
        )
    } else if model_lower.starts_with("bedrock/") {
        format!(
            "Model '{model}' requires AWS Bedrock credentials. \
             Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY (or use AWS_PROFILE)."
        )
    } else if model_lower.starts_with("openrouter/") {
        format!(
            "Model '{model}' requires OpenRouter credentials. \
             Set OPENROUTER_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("mistral/") {
        format!(
            "Model '{model}' requires Mistral credentials. \
             Set MISTRAL_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("groq/") {
        format!(
            "Model '{model}' requires Groq credentials. \
             Set GROQ_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else if model_lower.starts_with("deepseek")
        || model_lower.starts_with("kimi")
        || model_lower.starts_with("gpt-5")
    {
        format!(
            "Model '{model}' requires OpenCode Zen credentials. \
             Set OPENCODE_API_KEY or save a key to ~/.icode/auth.json."
        )
    } else {
        format!(
            "No API provider is configured for model '{model}'. \
             Open the Providers dialog (Ctrl+P → providers) or set one of: \
             ANTHROPIC_API_KEY, OPENAI_API_KEY, XAI_API_KEY, GEMINI_API_KEY, \
             GROQ_API_KEY, MISTRAL_API_KEY, OPENROUTER_API_KEY, OPENCODE_API_KEY."
        )
    }
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<String> {
        match self {
            Self::Anthropic(stream) => stream.request_id(),
            Self::OpenAiCompat(stream) => stream.request_id(),
            Self::Azure(stream) => stream.request_id(),
            Self::Gemini(stream) => stream.request_id(),
            Self::Bedrock(stream) => stream.request_id(),
            Self::OpenRouter(stream) => stream.request_id(),
            Self::Mistral(stream) => stream.request_id(),
            Self::Groq(stream) => stream.request_id(),
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        match self {
            Self::Anthropic(stream) => stream.next_event().await,
            Self::OpenAiCompat(stream) => stream.next_event().await,
            Self::Azure(stream) => stream.next_event().await,
            Self::Gemini(stream) => stream.next_event().await,
            Self::Bedrock(stream) => stream.next_event().await,
            Self::OpenRouter(stream) => stream.next_event().await,
            Self::Mistral(stream) => stream.next_event().await,
            Self::Groq(stream) => stream.next_event().await,
        }
    }
}

pub use anthropic::{
    oauth_token_is_expired, resolve_saved_oauth_token, resolve_startup_auth_source, OAuthTokenSet,
};
#[must_use]
pub fn read_base_url() -> String {
    anthropic::read_base_url()
}

#[must_use]
pub fn read_xai_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::xai())
}

#[must_use]
pub fn read_qwen_proxy_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::qwen_proxy())
}

#[cfg(test)]
mod tests {
    use crate::providers::{detect_provider_kind, resolve_model_alias, ProviderKind};

    #[test]
    fn resolves_existing_and_grok_aliases() {
        // With the catalog, aliases are passed through unchanged (catalog is source of truth)
        assert_eq!(resolve_model_alias("anthropic/claude-opus-4-5"), "anthropic/claude-opus-4-5");
        assert_eq!(resolve_model_alias("xai/grok-3"), "xai/grok-3");
    }

    #[test]
    fn provider_detection_prefers_model_family() {
        assert_eq!(detect_provider_kind("xai/grok-3", None), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("anthropic/claude-sonnet-4-5", None),
            ProviderKind::Anthropic
        );
    }
}
