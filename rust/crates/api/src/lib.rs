mod client;
mod error;
mod prompt_cache;
pub mod providers;
mod sse;
mod types;

pub use client::{
    oauth_token_is_expired, read_base_url, read_qwen_proxy_base_url, read_xai_base_url,
    resolve_saved_oauth_token, resolve_startup_auth_source, MessageStream, OAuthTokenSet,
    ProviderClient,
};
pub use error::ApiError;
pub use prompt_cache::{
    CacheBreakEvent, PromptCache, PromptCacheConfig, PromptCachePaths, PromptCacheRecord,
    PromptCacheStats,
};
pub use providers::anthropic::{AnthropicClient, AnthropicClient as ApiClient, AuthSource};
// pub use providers::azure::AzureClient;
// pub use providers::bedrock::BedrockClient;
// pub use providers::gemini::GeminiClient;
// pub use providers::groq::GroqClient;
// pub use providers::mistral::MistralClient;
pub use providers::openai_compat::{OpenAiCompatClient, OpenAiCompatConfig};
// pub use providers::openrouter::OpenRouterClient;
pub use providers::{
    capabilities_for_model, check_provider_auth, detect_provider_kind, is_provider_configured,
    list_all_models, max_tokens_for_model, provider_display_name, resolve_model_alias,
    scan_provider_auth_status, ModelCapabilities, ProviderKind, RegistryEntry,
};
pub use sse::{parse_frame, SseParser};
pub use types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

pub use telemetry::{
    AnalyticsEvent, AnthropicRequestProfile, ClientIdentity, JsonlTelemetrySink,
    MemoryTelemetrySink, SessionTraceRecord, SessionTracer, TelemetryEvent, TelemetrySink,
    DEFAULT_ANTHROPIC_VERSION,
};
