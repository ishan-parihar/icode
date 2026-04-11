use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use serde_json::{json, Value};

use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
use crate::permissions::PermissionOverride;

pub type HookPermissionDecision = PermissionOverride;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    SystemPromptTransform,
    ChatParamsTransform,
    RequestHeaders,
    ToolDefinitionTransform,
    ShellEnvInject,
}

impl HookEvent {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::SystemPromptTransform => "SystemPromptTransform",
            Self::ChatParamsTransform => "ChatParamsTransform",
            Self::RequestHeaders => "RequestHeaders",
            Self::ToolDefinitionTransform => "ToolDefinitionTransform",
            Self::ShellEnvInject => "ShellEnvInject",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookProgressEvent {
    Started {
        event: HookEvent,
        tool_name: String,
        command: String,
    },
    Completed {
        event: HookEvent,
        tool_name: String,
        command: String,
    },
    Cancelled {
        event: HookEvent,
        tool_name: String,
        command: String,
    },
}

pub trait HookProgressReporter: Send {
    fn on_event(&mut self, event: &HookProgressEvent);
}

#[derive(Debug, Clone, Default)]
pub struct HookAbortSignal {
    aborted: Arc<AtomicBool>,
}

impl HookAbortSignal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRunResult {
    denied: bool,
    failed: bool,
    cancelled: bool,
    messages: Vec<String>,
    permission_override: Option<PermissionOverride>,
    permission_reason: Option<String>,
    updated_input: Option<String>,
}

impl HookRunResult {
    #[must_use]
    pub fn allow(messages: Vec<String>) -> Self {
        Self {
            denied: false,
            failed: false,
            cancelled: false,
            messages,
            permission_override: None,
            permission_reason: None,
            updated_input: None,
        }
    }

    #[must_use]
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.failed
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    #[must_use]
    pub fn permission_override(&self) -> Option<PermissionOverride> {
        self.permission_override
    }

    #[must_use]
    pub fn permission_decision(&self) -> Option<HookPermissionDecision> {
        self.permission_override
    }

    #[must_use]
    pub fn permission_reason(&self) -> Option<&str> {
        self.permission_reason.as_deref()
    }

    #[must_use]
    pub fn updated_input(&self) -> Option<&str> {
        self.updated_input.as_deref()
    }

    #[must_use]
    pub fn updated_input_json(&self) -> Option<&str> {
        self.updated_input()
    }
}

// --- Input/Output types for transformation hooks ---

/// Result returned by a `SystemPromptTransform` hook.
/// The `updated_input` field in `HookRunResult` carries the transformed system prompt as a JSON string.
#[derive(Debug, Clone, Default)]
pub struct SystemPromptTransformInput {
    pub system_prompt: String,
}

impl SystemPromptTransformInput {
    #[must_use]
    pub fn to_json(&self) -> String {
        json!({
            "system_prompt": self.system_prompt,
        })
        .to_string()
    }
}

/// Result returned by a `ChatParamsTransform` hook.
/// The `updated_input` field carries the transformed chat params as a JSON string.
#[derive(Debug, Clone, Default)]
pub struct ChatParamsTransformInput {
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub top_p: Option<f64>,
}

impl ChatParamsTransformInput {
    #[must_use]
    pub fn to_json(&self) -> String {
        json!({
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "top_p": self.top_p,
        })
        .to_string()
    }
}

/// Result returned by a `RequestHeaders` hook.
/// The `updated_input` field carries additional headers as a JSON object string.
#[derive(Debug, Clone, Default)]
pub struct RequestHeadersInput {
    pub url: String,
    pub existing_headers: Vec<(String, String)>,
}

impl RequestHeadersInput {
    #[must_use]
    pub fn to_json(&self) -> String {
        json!({
            "url": self.url,
            "existing_headers": self.existing_headers.iter().map(|(k, v)| {
                json!({"key": k, "value": v})
            }).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

/// Input for a `ToolDefinitionTransform` hook.
/// The `updated_input` field carries the transformed tool definition as a JSON string.
#[derive(Debug, Clone, Default)]
pub struct ToolDefinitionTransformInput {
    pub tool_name: String,
    pub tool_definition: String,
}

impl ToolDefinitionTransformInput {
    #[must_use]
    pub fn to_json(&self) -> String {
        json!({
            "tool_name": self.tool_name,
            "tool_definition": serde_json::from_str::<Value>(&self.tool_definition)
                .unwrap_or_else(|_| json!({ "raw": self.tool_definition })),
        })
        .to_string()
    }
}

/// Input/Result for a `ShellEnvInject` hook.
/// The `updated_input` field carries environment variables to inject as a JSON object string.
#[derive(Debug, Clone, Default)]
pub struct ShellEnvInjectInput {
    pub command: String,
    pub existing_env: Vec<(String, String)>,
}

impl ShellEnvInjectInput {
    #[must_use]
    pub fn to_json(&self) -> String {
        json!({
            "command": self.command,
            "existing_env": self.existing_env.iter().map(|(k, v)| {
                json!({"key": k, "value": v})
            }).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookRunner {
    config: RuntimeHookConfig,
}

impl HookRunner {
    #[must_use]
    pub fn new(config: RuntimeHookConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn from_feature_config(feature_config: &RuntimeFeatureConfig) -> Self {
        Self::new(feature_config.hooks().clone())
    }

    #[must_use]
    pub fn run_pre_tool_use(&self, tool_name: &str, tool_input: &str) -> HookRunResult {
        self.run_pre_tool_use_with_context(tool_name, tool_input, None, None)
    }

    #[must_use]
    pub fn run_pre_tool_use_with_context(
        &self,
        tool_name: &str,
        tool_input: &str,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        Self::run_commands(
            HookEvent::PreToolUse,
            self.config.pre_tool_use(),
            tool_name,
            tool_input,
            None,
            false,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_pre_tool_use_with_signal(
        &self,
        tool_name: &str,
        tool_input: &str,
        abort_signal: Option<&HookAbortSignal>,
    ) -> HookRunResult {
        self.run_pre_tool_use_with_context(tool_name, tool_input, abort_signal, None)
    }

    #[must_use]
    pub fn run_post_tool_use(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_output: &str,
        is_error: bool,
    ) -> HookRunResult {
        self.run_post_tool_use_with_context(
            tool_name,
            tool_input,
            tool_output,
            is_error,
            None,
            None,
        )
    }

    #[must_use]
    pub fn run_post_tool_use_with_context(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_output: &str,
        is_error: bool,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        Self::run_commands(
            HookEvent::PostToolUse,
            self.config.post_tool_use(),
            tool_name,
            tool_input,
            Some(tool_output),
            is_error,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_post_tool_use_with_signal(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_output: &str,
        is_error: bool,
        abort_signal: Option<&HookAbortSignal>,
    ) -> HookRunResult {
        self.run_post_tool_use_with_context(
            tool_name,
            tool_input,
            tool_output,
            is_error,
            abort_signal,
            None,
        )
    }

    #[must_use]
    pub fn run_post_tool_use_failure(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_error: &str,
    ) -> HookRunResult {
        self.run_post_tool_use_failure_with_context(tool_name, tool_input, tool_error, None, None)
    }

    #[must_use]
    pub fn run_post_tool_use_failure_with_context(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_error: &str,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        Self::run_commands(
            HookEvent::PostToolUseFailure,
            self.config.post_tool_use_failure(),
            tool_name,
            tool_input,
            Some(tool_error),
            true,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_post_tool_use_failure_with_signal(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_error: &str,
        abort_signal: Option<&HookAbortSignal>,
    ) -> HookRunResult {
        self.run_post_tool_use_failure_with_context(
            tool_name,
            tool_input,
            tool_error,
            abort_signal,
            None,
        )
    }

    #[must_use]
    pub fn run_system_prompt_transform(&self, input: &SystemPromptTransformInput) -> HookRunResult {
        self.run_system_prompt_transform_with_context(input, None, None)
    }

    #[must_use]
    pub fn run_system_prompt_transform_with_context(
        &self,
        input: &SystemPromptTransformInput,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        let payload = input.to_json();
        Self::run_transform_commands(
            HookEvent::SystemPromptTransform,
            self.config.system_prompt_transform(),
            "SystemPromptTransform",
            &payload,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_chat_params_transform(&self, input: &ChatParamsTransformInput) -> HookRunResult {
        self.run_chat_params_transform_with_context(input, None, None)
    }

    #[must_use]
    pub fn run_chat_params_transform_with_context(
        &self,
        input: &ChatParamsTransformInput,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        let payload = input.to_json();
        Self::run_transform_commands(
            HookEvent::ChatParamsTransform,
            self.config.chat_params_transform(),
            "ChatParamsTransform",
            &payload,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_request_headers(&self, input: &RequestHeadersInput) -> HookRunResult {
        self.run_request_headers_with_context(input, None, None)
    }

    #[must_use]
    pub fn run_request_headers_with_context(
        &self,
        input: &RequestHeadersInput,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        let payload = input.to_json();
        Self::run_transform_commands(
            HookEvent::RequestHeaders,
            self.config.request_headers(),
            "RequestHeaders",
            &payload,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_tool_definition_transform(
        &self,
        input: &ToolDefinitionTransformInput,
    ) -> HookRunResult {
        self.run_tool_definition_transform_with_context(input, None, None)
    }

    #[must_use]
    pub fn run_tool_definition_transform_with_context(
        &self,
        input: &ToolDefinitionTransformInput,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        let payload = input.to_json();
        Self::run_transform_commands(
            HookEvent::ToolDefinitionTransform,
            self.config.tool_definition_transform(),
            "ToolDefinitionTransform",
            &payload,
            abort_signal,
            reporter,
        )
    }

    #[must_use]
    pub fn run_shell_env_inject(&self, input: &ShellEnvInjectInput) -> HookRunResult {
        self.run_shell_env_inject_with_context(input, None, None)
    }

    #[must_use]
    pub fn run_shell_env_inject_with_context(
        &self,
        input: &ShellEnvInjectInput,
        abort_signal: Option<&HookAbortSignal>,
        reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        let payload = input.to_json();
        Self::run_transform_commands(
            HookEvent::ShellEnvInject,
            self.config.shell_env_inject(),
            "ShellEnvInject",
            &payload,
            abort_signal,
            reporter,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_commands(
        event: HookEvent,
        commands: &[String],
        tool_name: &str,
        tool_input: &str,
        tool_output: Option<&str>,
        is_error: bool,
        abort_signal: Option<&HookAbortSignal>,
        mut reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        if commands.is_empty() {
            return HookRunResult::allow(Vec::new());
        }

        if abort_signal.is_some_and(HookAbortSignal::is_aborted) {
            return HookRunResult {
                denied: false,
                failed: false,
                cancelled: true,
                messages: vec![format!(
                    "{} hook cancelled before execution",
                    event.as_str()
                )],
                permission_override: None,
                permission_reason: None,
                updated_input: None,
            };
        }

        let payload = hook_payload(event, tool_name, tool_input, tool_output, is_error).to_string();
        let mut result = HookRunResult::allow(Vec::new());

        for command in commands {
            if let Some(reporter) = reporter.as_deref_mut() {
                reporter.on_event(&HookProgressEvent::Started {
                    event,
                    tool_name: tool_name.to_string(),
                    command: command.clone(),
                });
            }

            match Self::run_command(
                command,
                event,
                tool_name,
                tool_input,
                tool_output,
                is_error,
                &payload,
                abort_signal,
            ) {
                HookCommandOutcome::Allow { parsed } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Completed {
                            event,
                            tool_name: tool_name.to_string(),
                            command: command.clone(),
                        });
                    }
                    merge_parsed_hook_output(&mut result, parsed);
                }
                HookCommandOutcome::Deny { parsed } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Completed {
                            event,
                            tool_name: tool_name.to_string(),
                            command: command.clone(),
                        });
                    }
                    merge_parsed_hook_output(&mut result, parsed);
                    result.denied = true;
                    return result;
                }
                HookCommandOutcome::Failed { parsed } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Completed {
                            event,
                            tool_name: tool_name.to_string(),
                            command: command.clone(),
                        });
                    }
                    merge_parsed_hook_output(&mut result, parsed);
                    result.failed = true;
                    return result;
                }
                HookCommandOutcome::Cancelled { message } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Cancelled {
                            event,
                            tool_name: tool_name.to_string(),
                            command: command.clone(),
                        });
                    }
                    result.cancelled = true;
                    result.messages.push(message);
                    return result;
                }
            }
        }

        result
    }

    fn run_transform_commands(
        event: HookEvent,
        commands: &[String],
        hook_label: &str,
        payload: &str,
        abort_signal: Option<&HookAbortSignal>,
        mut reporter: Option<&mut dyn HookProgressReporter>,
    ) -> HookRunResult {
        if commands.is_empty() {
            return HookRunResult::allow(Vec::new());
        }

        if abort_signal.is_some_and(HookAbortSignal::is_aborted) {
            return HookRunResult {
                denied: false,
                failed: false,
                cancelled: true,
                messages: vec![format!(
                    "{} hook cancelled before execution",
                    event.as_str()
                )],
                permission_override: None,
                permission_reason: None,
                updated_input: None,
            };
        }

        let mut result = HookRunResult::allow(Vec::new());

        for command in commands {
            if let Some(reporter) = reporter.as_deref_mut() {
                reporter.on_event(&HookProgressEvent::Started {
                    event,
                    tool_name: hook_label.to_string(),
                    command: command.clone(),
                });
            }

            match Self::run_transform_command(command, event, hook_label, payload, abort_signal) {
                HookCommandOutcome::Allow { parsed } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Completed {
                            event,
                            tool_name: hook_label.to_string(),
                            command: command.clone(),
                        });
                    }
                    merge_parsed_hook_output(&mut result, parsed);
                }
                HookCommandOutcome::Failed { parsed } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Completed {
                            event,
                            tool_name: hook_label.to_string(),
                            command: command.clone(),
                        });
                    }
                    merge_parsed_hook_output(&mut result, parsed);
                    result.failed = true;
                    return result;
                }
                HookCommandOutcome::Cancelled { message } => {
                    if let Some(reporter) = reporter.as_deref_mut() {
                        reporter.on_event(&HookProgressEvent::Cancelled {
                            event,
                            tool_name: hook_label.to_string(),
                            command: command.clone(),
                        });
                    }
                    result.cancelled = true;
                    result.messages.push(message);
                    return result;
                }
                HookCommandOutcome::Deny { parsed } => {
                    merge_parsed_hook_output(&mut result, parsed);
                    result.denied = true;
                    return result;
                }
            }
        }

        result
    }

    fn run_transform_command(
        command: &str,
        event: HookEvent,
        hook_label: &str,
        payload: &str,
        abort_signal: Option<&HookAbortSignal>,
    ) -> HookCommandOutcome {
        let mut child = shell_command(command);
        child.stdin(Stdio::piped());
        child.stdout(Stdio::piped());
        child.stderr(Stdio::piped());
        child.env("HOOK_EVENT", event.as_str());
        child.env("HOOK_EVENT_TYPE", hook_label);

        match child.output_with_stdin(payload.as_bytes(), abort_signal) {
            Ok(CommandExecution::Finished(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let parsed = parse_hook_output(&stdout);
                let primary_message = parsed.primary_message().map(ToOwned::to_owned);
                match output.status.code() {
                    Some(0) => HookCommandOutcome::Allow { parsed },
                    Some(2) => HookCommandOutcome::Deny {
                        parsed: parsed
                            .with_fallback_message(format!("{} hook denied", event.as_str())),
                    },
                    Some(code) => HookCommandOutcome::Failed {
                        parsed: parsed.with_fallback_message(format_hook_failure(
                            command,
                            code,
                            primary_message.as_deref(),
                            stderr.as_str(),
                        )),
                    },
                    None => HookCommandOutcome::Failed {
                        parsed: parsed.with_fallback_message(format!(
                            "{} hook `{command}` terminated by signal",
                            event.as_str()
                        )),
                    },
                }
            }
            Ok(CommandExecution::Cancelled) => HookCommandOutcome::Cancelled {
                message: format!("{} hook `{command}` cancelled", event.as_str()),
            },
            Err(error) => HookCommandOutcome::Failed {
                parsed: ParsedHookOutput {
                    messages: vec![format!(
                        "{} hook `{command}` failed to start: {error}",
                        event.as_str()
                    )],
                    ..ParsedHookOutput::default()
                },
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_command(
        command: &str,
        event: HookEvent,
        tool_name: &str,
        tool_input: &str,
        tool_output: Option<&str>,
        is_error: bool,
        payload: &str,
        abort_signal: Option<&HookAbortSignal>,
    ) -> HookCommandOutcome {
        let mut child = shell_command(command);
        child.stdin(Stdio::piped());
        child.stdout(Stdio::piped());
        child.stderr(Stdio::piped());
        child.env("HOOK_EVENT", event.as_str());
        child.env("HOOK_TOOL_NAME", tool_name);
        child.env("HOOK_TOOL_INPUT", tool_input);
        child.env("HOOK_TOOL_IS_ERROR", if is_error { "1" } else { "0" });
        if let Some(tool_output) = tool_output {
            child.env("HOOK_TOOL_OUTPUT", tool_output);
        }

        match child.output_with_stdin(payload.as_bytes(), abort_signal) {
            Ok(CommandExecution::Finished(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let parsed = parse_hook_output(&stdout);
                let primary_message = parsed.primary_message().map(ToOwned::to_owned);
                match output.status.code() {
                    Some(0) => {
                        if parsed.deny {
                            HookCommandOutcome::Deny { parsed }
                        } else {
                            HookCommandOutcome::Allow { parsed }
                        }
                    }
                    Some(2) => HookCommandOutcome::Deny {
                        parsed: parsed.with_fallback_message(format!(
                            "{} hook denied tool `{tool_name}`",
                            event.as_str()
                        )),
                    },
                    Some(code) => HookCommandOutcome::Failed {
                        parsed: parsed.with_fallback_message(format_hook_failure(
                            command,
                            code,
                            primary_message.as_deref(),
                            stderr.as_str(),
                        )),
                    },
                    None => HookCommandOutcome::Failed {
                        parsed: parsed.with_fallback_message(format!(
                            "{} hook `{command}` terminated by signal while handling `{}`",
                            event.as_str(),
                            tool_name
                        )),
                    },
                }
            }
            Ok(CommandExecution::Cancelled) => HookCommandOutcome::Cancelled {
                message: format!(
                    "{} hook `{command}` cancelled while handling `{tool_name}`",
                    event.as_str()
                ),
            },
            Err(error) => HookCommandOutcome::Failed {
                parsed: ParsedHookOutput {
                    messages: vec![format!(
                        "{} hook `{command}` failed to start for `{}`: {error}",
                        event.as_str(),
                        tool_name
                    )],
                    ..ParsedHookOutput::default()
                },
            },
        }
    }
}

enum HookCommandOutcome {
    Allow { parsed: ParsedHookOutput },
    Deny { parsed: ParsedHookOutput },
    Failed { parsed: ParsedHookOutput },
    Cancelled { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ParsedHookOutput {
    messages: Vec<String>,
    deny: bool,
    permission_override: Option<PermissionOverride>,
    permission_reason: Option<String>,
    updated_input: Option<String>,
}

impl ParsedHookOutput {
    fn with_fallback_message(mut self, fallback: String) -> Self {
        if self.messages.is_empty() {
            self.messages.push(fallback);
        }
        self
    }

    fn primary_message(&self) -> Option<&str> {
        self.messages.first().map(String::as_str)
    }
}

fn merge_parsed_hook_output(target: &mut HookRunResult, parsed: ParsedHookOutput) {
    target.messages.extend(parsed.messages);
    if parsed.permission_override.is_some() {
        target.permission_override = parsed.permission_override;
    }
    if parsed.permission_reason.is_some() {
        target.permission_reason = parsed.permission_reason;
    }
    if parsed.updated_input.is_some() {
        target.updated_input = parsed.updated_input;
    }
}

fn parse_hook_output(stdout: &str) -> ParsedHookOutput {
    if stdout.is_empty() {
        return ParsedHookOutput::default();
    }

    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(stdout) else {
        return ParsedHookOutput {
            messages: vec![stdout.to_string()],
            ..ParsedHookOutput::default()
        };
    };

    let mut parsed = ParsedHookOutput::default();

    if let Some(message) = root.get("systemMessage").and_then(Value::as_str) {
        parsed.messages.push(message.to_string());
    }
    if let Some(message) = root.get("reason").and_then(Value::as_str) {
        parsed.messages.push(message.to_string());
    }
    if root.get("continue").and_then(Value::as_bool) == Some(false)
        || root.get("decision").and_then(Value::as_str) == Some("block")
    {
        parsed.deny = true;
    }

    if let Some(Value::Object(specific)) = root.get("hookSpecificOutput") {
        if let Some(Value::String(additional_context)) = specific.get("additionalContext") {
            parsed.messages.push(additional_context.clone());
        }
        if let Some(decision) = specific.get("permissionDecision").and_then(Value::as_str) {
            parsed.permission_override = match decision {
                "allow" => Some(PermissionOverride::Allow),
                "deny" => Some(PermissionOverride::Deny),
                "ask" => Some(PermissionOverride::Ask),
                _ => None,
            };
        }
        if let Some(reason) = specific
            .get("permissionDecisionReason")
            .and_then(Value::as_str)
        {
            parsed.permission_reason = Some(reason.to_string());
        }
        if let Some(updated_input) = specific.get("updatedInput") {
            match serde_json::to_string(updated_input) {
                Ok(s) => parsed.updated_input = Some(s),
                Err(e) => {
                    tracing::warn!("hooks: failed to serialize hook updatedInput, input unchanged: {e}");
                }
            }
        }
    }

    if parsed.messages.is_empty() {
        parsed.messages.push(stdout.to_string());
    }

    parsed
}

fn hook_payload(
    event: HookEvent,
    tool_name: &str,
    tool_input: &str,
    tool_output: Option<&str>,
    is_error: bool,
) -> Value {
    match event {
        HookEvent::PostToolUseFailure => json!({
            "hook_event_name": event.as_str(),
            "tool_name": tool_name,
            "tool_input": parse_tool_input(tool_input),
            "tool_input_json": tool_input,
            "tool_error": tool_output,
            "tool_result_is_error": true,
        }),
        _ => json!({
            "hook_event_name": event.as_str(),
            "tool_name": tool_name,
            "tool_input": parse_tool_input(tool_input),
            "tool_input_json": tool_input,
            "tool_output": tool_output,
            "tool_result_is_error": is_error,
        }),
    }
}

fn parse_tool_input(tool_input: &str) -> Value {
    serde_json::from_str(tool_input).unwrap_or_else(|_| json!({ "raw": tool_input }))
}

fn format_hook_failure(command: &str, code: i32, stdout: Option<&str>, stderr: &str) -> String {
    let mut message = format!("Hook `{command}` exited with status {code}");
    if let Some(stdout) = stdout.filter(|stdout| !stdout.is_empty()) {
        message.push_str(": ");
        message.push_str(stdout);
    } else if !stderr.is_empty() {
        message.push_str(": ");
        message.push_str(stderr);
    }
    message
}

fn shell_command(command: &str) -> CommandWithStdin {
    #[cfg(windows)]
    let mut command_builder = {
        let mut command_builder = Command::new("cmd");
        command_builder.arg("/C").arg(command);
        CommandWithStdin::new(command_builder)
    };

    #[cfg(not(windows))]
    let command_builder = {
        let mut command_builder = Command::new("sh");
        command_builder.arg("-lc").arg(command);
        CommandWithStdin::new(command_builder)
    };

    command_builder
}

struct CommandWithStdin {
    command: Command,
}

impl CommandWithStdin {
    fn new(command: Command) -> Self {
        Self { command }
    }

    fn stdin(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stdin(cfg);
        self
    }

    fn stdout(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stdout(cfg);
        self
    }

    fn stderr(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stderr(cfg);
        self
    }

    fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, value);
        self
    }

    fn output_with_stdin(
        &mut self,
        stdin: &[u8],
        abort_signal: Option<&HookAbortSignal>,
    ) -> std::io::Result<CommandExecution> {
        let mut child = self.command.spawn()?;
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(stdin)?;
        }

        let deadline = Instant::now() + Duration::from_secs(30);

        loop {
            if abort_signal.is_some_and(HookAbortSignal::is_aborted) {
                if let Err(e) = child.kill() {
                    tracing::warn!("hooks: child kill failed: {e}");
                }
                if let Err(e) = child.wait_with_output() {
                    tracing::warn!("hooks: child reap failed: {e}");
                }
                return Ok(CommandExecution::Cancelled);
            }

            if Instant::now() > deadline {
                if let Err(e) = child.kill() {
                    tracing::warn!("hooks: child kill failed: {e}");
                }
                if let Err(e) = child.wait_with_output() {
                    tracing::warn!("hooks: child reap failed: {e}");
                }
                return Ok(CommandExecution::Cancelled);
            }

            match child.try_wait()? {
                Some(_) => return child.wait_with_output().map(CommandExecution::Finished),
                None => thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

enum CommandExecution {
    Finished(std::process::Output),
    Cancelled,
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use super::{
        HookAbortSignal, HookEvent, HookProgressEvent, HookProgressReporter, HookRunResult,
        HookRunner,
    };
    use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
    use crate::permissions::PermissionOverride;

    struct RecordingReporter {
        events: Vec<HookProgressEvent>,
    }

    impl HookProgressReporter for RecordingReporter {
        fn on_event(&mut self, event: &HookProgressEvent) {
            self.events.push(event.clone());
        }
    }

    #[test]
    fn allows_exit_code_zero_and_captures_stdout() {
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![shell_snippet("printf 'pre ok'")],
            Vec::new(),
            Vec::new(),
        ));

        let result = runner.run_pre_tool_use("Read", r#"{"path":"README.md"}"#);

        assert_eq!(result, HookRunResult::allow(vec!["pre ok".to_string()]));
    }

    #[test]
    fn denies_exit_code_two() {
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![shell_snippet("printf 'blocked by hook'; exit 2")],
            Vec::new(),
            Vec::new(),
        ));

        let result = runner.run_pre_tool_use("Bash", r#"{"command":"pwd"}"#);

        assert!(result.is_denied());
        assert_eq!(result.messages(), &["blocked by hook".to_string()]);
    }

    #[test]
    fn propagates_other_non_zero_statuses_as_failures() {
        let runner = HookRunner::from_feature_config(&RuntimeFeatureConfig::default().with_hooks(
            RuntimeHookConfig::new(
                vec![shell_snippet("printf 'warning hook'; exit 1")],
                Vec::new(),
                Vec::new(),
            ),
        ));

        // given
        // when
        let result = runner.run_pre_tool_use("Edit", r#"{"file":"src/lib.rs"}"#);

        // then
        assert!(result.is_failed());
        assert!(result
            .messages()
            .iter()
            .any(|message| message.contains("warning hook")));
    }

    #[test]
    fn parses_pre_hook_permission_override_and_updated_input() {
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![shell_snippet(
                r#"printf '%s' '{"systemMessage":"updated","hookSpecificOutput":{"permissionDecision":"allow","permissionDecisionReason":"hook ok","updatedInput":{"command":"git status"}}}'"#,
            )],
            Vec::new(),
            Vec::new(),
        ));

        let result = runner.run_pre_tool_use("bash", r#"{"command":"pwd"}"#);

        assert_eq!(
            result.permission_override(),
            Some(PermissionOverride::Allow)
        );
        assert_eq!(result.permission_reason(), Some("hook ok"));
        assert_eq!(result.updated_input(), Some(r#"{"command":"git status"}"#));
        assert!(result.messages().iter().any(|message| message == "updated"));
    }

    #[test]
    fn runs_post_tool_use_failure_hooks() {
        // given
        let runner = HookRunner::new(RuntimeHookConfig::new(
            Vec::new(),
            Vec::new(),
            vec![shell_snippet("printf 'failure hook ran'")],
        ));

        // when
        let result =
            runner.run_post_tool_use_failure("bash", r#"{"command":"false"}"#, "command failed");

        // then
        assert!(!result.is_denied());
        assert_eq!(result.messages(), &["failure hook ran".to_string()]);
    }

    #[test]
    fn stops_running_failure_hooks_after_failure() {
        // given
        let runner = HookRunner::new(RuntimeHookConfig::new(
            Vec::new(),
            Vec::new(),
            vec![
                shell_snippet("printf 'broken failure hook'; exit 1"),
                shell_snippet("printf 'later failure hook'"),
            ],
        ));

        // when
        let result =
            runner.run_post_tool_use_failure("bash", r#"{"command":"false"}"#, "command failed");

        // then
        assert!(result.is_failed());
        assert!(result
            .messages()
            .iter()
            .any(|message| message.contains("broken failure hook")));
        assert!(!result
            .messages()
            .iter()
            .any(|message| message == "later failure hook"));
    }

    #[test]
    fn executes_hooks_in_configured_order() {
        // given
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![
                shell_snippet("printf 'first'"),
                shell_snippet("printf 'second'"),
            ],
            Vec::new(),
            Vec::new(),
        ));
        let mut reporter = RecordingReporter { events: Vec::new() };

        // when
        let result = runner.run_pre_tool_use_with_context(
            "Read",
            r#"{"path":"README.md"}"#,
            None,
            Some(&mut reporter),
        );

        // then
        assert_eq!(
            result,
            HookRunResult::allow(vec!["first".to_string(), "second".to_string()])
        );
        assert_eq!(reporter.events.len(), 4);
        assert!(matches!(
            &reporter.events[0],
            HookProgressEvent::Started {
                event: HookEvent::PreToolUse,
                command,
                ..
            } if command == "printf 'first'"
        ));
        assert!(matches!(
            &reporter.events[1],
            HookProgressEvent::Completed {
                event: HookEvent::PreToolUse,
                command,
                ..
            } if command == "printf 'first'"
        ));
        assert!(matches!(
            &reporter.events[2],
            HookProgressEvent::Started {
                event: HookEvent::PreToolUse,
                command,
                ..
            } if command == "printf 'second'"
        ));
        assert!(matches!(
            &reporter.events[3],
            HookProgressEvent::Completed {
                event: HookEvent::PreToolUse,
                command,
                ..
            } if command == "printf 'second'"
        ));
    }

    #[test]
    fn stops_running_hooks_after_failure() {
        // given
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![
                shell_snippet("printf 'broken'; exit 1"),
                shell_snippet("printf 'later'"),
            ],
            Vec::new(),
            Vec::new(),
        ));

        // when
        let result = runner.run_pre_tool_use("Edit", r#"{"file":"src/lib.rs"}"#);

        // then
        assert!(result.is_failed());
        assert!(result
            .messages()
            .iter()
            .any(|message| message.contains("broken")));
        assert!(!result.messages().iter().any(|message| message == "later"));
    }

    #[test]
    fn abort_signal_cancels_long_running_hook_and_reports_progress() {
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![shell_snippet("sleep 5")],
            Vec::new(),
            Vec::new(),
        ));
        let abort_signal = HookAbortSignal::new();
        let abort_signal_for_thread = abort_signal.clone();
        let mut reporter = RecordingReporter { events: Vec::new() };

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            abort_signal_for_thread.abort();
        });

        let result = runner.run_pre_tool_use_with_context(
            "bash",
            r#"{"command":"sleep 5"}"#,
            Some(&abort_signal),
            Some(&mut reporter),
        );

        assert!(result.is_cancelled());
        assert!(reporter.events.iter().any(|event| matches!(
            event,
            HookProgressEvent::Started {
                event: HookEvent::PreToolUse,
                ..
            }
        )));
        assert!(reporter.events.iter().any(|event| matches!(
            event,
            HookProgressEvent::Cancelled {
                event: HookEvent::PreToolUse,
                ..
            }
        )));
    }

    #[cfg(windows)]
    fn shell_snippet(script: &str) -> String {
        script.replace('\'', "\"")
    }

    #[cfg(not(windows))]
    fn shell_snippet(script: &str) -> String {
        script.to_string()
    }

    fn transform_config() -> RuntimeHookConfig {
        RuntimeHookConfig::with_transform_hooks(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![shell_snippet(
                "printf '{\"hookSpecificOutput\":{\"updatedInput\":\"transformed prompt\"}}'",
            )],
            vec![shell_snippet(
                "printf '{\"hookSpecificOutput\":{\"updatedInput\":{\"model\":\"test\"}}}'",
            )],
            vec![shell_snippet(
                "printf '{\"hookSpecificOutput\":{\"updatedInput\":{\"X-Custom\":\"value\"}}}'",
            )],
            vec![shell_snippet(
                "printf '{\"hookSpecificOutput\":{\"updatedInput\":{\"name\":\"test\",\"description\":\"t\"}}}'",
            )],
            vec![shell_snippet(
                "printf '{\"hookSpecificOutput\":{\"updatedInput\":{\"PATH\":\"/custom\"}}}'",
            )],
        )
    }

    #[test]
    fn system_prompt_transform_returns_updated_input() {
        let runner = HookRunner::new(transform_config());
        let input = super::SystemPromptTransformInput {
            system_prompt: "You are a helpful assistant.".to_string(),
        };

        let result = runner.run_system_prompt_transform(&input);

        assert!(!result.is_denied());
        assert!(!result.is_failed());
        assert_eq!(result.updated_input(), Some("\"transformed prompt\""));
    }

    #[test]
    fn chat_params_transform_returns_updated_input() {
        let runner = HookRunner::new(transform_config());
        let input = super::ChatParamsTransformInput {
            model: "claude-sonnet-4-6".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: None,
        };

        let result = runner.run_chat_params_transform(&input);

        assert!(!result.is_failed());
        assert!(result.updated_input().is_some());
    }

    #[test]
    fn request_headers_transform_returns_updated_input() {
        let runner = HookRunner::new(transform_config());
        let input = super::RequestHeadersInput {
            url: "https://api.example.com".to_string(),
            existing_headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
        };

        let result = runner.run_request_headers(&input);

        assert!(!result.is_failed());
        assert!(result.updated_input().is_some());
    }

    #[test]
    fn tool_definition_transform_returns_updated_input() {
        let runner = HookRunner::new(transform_config());
        let input = super::ToolDefinitionTransformInput {
            tool_name: "bash".to_string(),
            tool_definition: r#"{"name":"bash","description":"Run a command"}"#.to_string(),
        };

        let result = runner.run_tool_definition_transform(&input);

        assert!(!result.is_failed());
        assert!(result.updated_input().is_some());
    }

    #[test]
    fn shell_env_inject_returns_updated_input() {
        let runner = HookRunner::new(transform_config());
        let input = super::ShellEnvInjectInput {
            command: "ls -la".to_string(),
            existing_env: vec![("HOME".to_string(), "/home/user".to_string())],
        };

        let result = runner.run_shell_env_inject(&input);

        assert!(!result.is_failed());
        assert!(result.updated_input().is_some());
    }

    #[test]
    fn transform_hook_failure_stops_execution() {
        let config = RuntimeHookConfig::with_transform_hooks(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![shell_snippet("printf 'broken'; exit 1")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let runner = HookRunner::new(config);
        let input = super::SystemPromptTransformInput {
            system_prompt: "test".to_string(),
        };

        let result = runner.run_system_prompt_transform(&input);

        assert!(result.is_failed());
        assert!(result.messages().iter().any(|m| m.contains("broken")));
    }

    #[test]
    fn transform_hook_deny_on_exit_two() {
        let config = RuntimeHookConfig::with_transform_hooks(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![shell_snippet("printf 'denied'; exit 2")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let runner = HookRunner::new(config);
        let input = super::SystemPromptTransformInput {
            system_prompt: "test".to_string(),
        };

        let result = runner.run_system_prompt_transform(&input);

        assert!(result.is_denied());
        assert!(result.messages().iter().any(|m| m.contains("denied")));
    }

    #[test]
    fn empty_transform_config_returns_allow() {
        let runner = HookRunner::new(RuntimeHookConfig::default());
        let input = super::SystemPromptTransformInput {
            system_prompt: "test".to_string(),
        };

        let result = runner.run_system_prompt_transform(&input);

        assert!(!result.is_denied());
        assert!(!result.is_failed());
        assert!(result.messages().is_empty());
    }

    #[test]
    fn transform_hook_reports_progress() {
        let config = RuntimeHookConfig::with_transform_hooks(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![shell_snippet("printf 'ok'")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let runner = HookRunner::new(config);
        let mut reporter = RecordingReporter { events: Vec::new() };
        let input = super::SystemPromptTransformInput {
            system_prompt: "test".to_string(),
        };

        let _result =
            runner.run_system_prompt_transform_with_context(&input, None, Some(&mut reporter));

        assert_eq!(reporter.events.len(), 2);
        assert!(matches!(
            &reporter.events[0],
            HookProgressEvent::Started {
                event: HookEvent::SystemPromptTransform,
                ..
            }
        ));
        assert!(matches!(
            &reporter.events[1],
            HookProgressEvent::Completed {
                event: HookEvent::SystemPromptTransform,
                ..
            }
        ));
    }

    #[test]
    fn transform_hook_abort_signal_cancels() {
        let config = RuntimeHookConfig::with_transform_hooks(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![shell_snippet("sleep 5")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let runner = HookRunner::new(config);
        let abort_signal = HookAbortSignal::new();
        let abort_clone = abort_signal.clone();
        let mut reporter = RecordingReporter { events: Vec::new() };
        let input = super::SystemPromptTransformInput {
            system_prompt: "test".to_string(),
        };

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            abort_clone.abort();
        });

        let result = runner.run_system_prompt_transform_with_context(
            &input,
            Some(&abort_signal),
            Some(&mut reporter),
        );

        assert!(result.is_cancelled());
    }
}
