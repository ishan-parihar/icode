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
    Unconfigured,
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
        Self::from_model_with_anthropic_auth(model, None)
    }

    pub fn from_model_with_anthropic_auth(
        model: &str,
        anthropic_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        let resolved_model = providers::resolve_model_alias(model);
        let result = match providers::detect_provider_kind(&resolved_model) {
            ProviderKind::Anthropic => match anthropic_auth {
                Some(auth) => Ok(Self::Anthropic(AnthropicClient::from_auth(auth))),
                None => AnthropicClient::from_env().map(Self::Anthropic),
            },
            ProviderKind::Xai => OpenAiCompatClient::from_env(OpenAiCompatConfig::xai())
                .map(Self::Xai),
            ProviderKind::OpenAi => OpenAiCompatClient::from_env(OpenAiCompatConfig::openai())
                .map(Self::OpenAi),
            ProviderKind::QwenProxy => {
                let config = OpenAiCompatConfig::qwen_proxy();
                match std::env::var("QWEN_PROXY_API_KEY") {
                    Ok(api_key) => Ok(Self::QwenProxy(
                        OpenAiCompatClient::new(api_key, config)
                            .with_base_url(read_qwen_proxy_base_url()),
                    )),
                    Err(_) => Err(ApiError::missing_credentials(
                        "qwen-proxy",
                        &["QWEN_PROXY_API_KEY"],
                    )),
                }
            }
            ProviderKind::Azure => AzureClient::from_env().map(Self::Azure),
            ProviderKind::Gemini => GeminiClient::from_env().map(Self::Gemini),
            ProviderKind::Bedrock => BedrockClient::from_env().map(Self::Bedrock),
            ProviderKind::OpenRouter => OpenRouterClient::from_env().map(Self::OpenRouter),
            ProviderKind::Mistral => MistralClient::from_env().map(Self::Mistral),
            ProviderKind::Groq => GroqClient::from_env().map(Self::Groq),
            ProviderKind::Unconfigured => Ok(Self::Unconfigured),
        };
        // If credentials are missing, return Unconfigured instead of erroring.
        // This allows the TUI to start and lets users configure providers interactively.
        // The actual error is deferred until an API call is made.
        match result {
            Err(ApiError::MissingCredentials { .. } | ApiError::Auth(_)) => Ok(Self::Unconfigured),
            other => other,
        }
    }

    #[must_use]
    pub const fn provider_kind(&self) -> ProviderKind {
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
            Self::Unconfigured => ProviderKind::Unconfigured,
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
            Self::Xai(client) | Self::OpenAi(client) | Self::QwenProxy(client) => {
                client.send_message(request).await
            }
            Self::Azure(client) => client.send_message(request).await,
            Self::Gemini(client) => client.send_message(request).await,
            Self::Bedrock(client) => client.send_message(request).await,
            Self::OpenRouter(client) => client.send_message(request).await,
            Self::Mistral(client) => client.send_message(request).await,
            Self::Groq(client) => client.send_message(request).await,
            Self::Unconfigured => Err(ApiError::Auth(
                "No API provider configured. Set credentials via environment variables or ~/.icode/auth.json".to_string(),
            )),
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
            Self::Xai(client) | Self::OpenAi(client) | Self::QwenProxy(client) => client
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
            Self::Unconfigured => Err(ApiError::Auth(
                "No API provider configured. Set credentials via environment variables or ~/.icode/auth.json".to_string(),
            )),
        }
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
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
    }

    #[test]
    fn provider_detection_prefers_model_family() {
        assert_eq!(detect_provider_kind("grok-3"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::Anthropic
        );
    }
}
