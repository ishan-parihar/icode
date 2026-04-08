use std::collections::HashMap;
use std::fmt::Write;

use crate::tui::app::{AppState, MessagePart, MessageRole, ToolStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptFormat {
    Plain,
    Markdown,
    Json,
}

pub fn export_transcript(state: &AppState, format: TranscriptFormat) -> Result<String, String> {
    match format {
        TranscriptFormat::Plain => format_plain(state),
        TranscriptFormat::Markdown => format_markdown(state),
        TranscriptFormat::Json => format_json(state),
    }
}

fn format_plain(state: &AppState) -> Result<String, String> {
    let mut output = String::new();

    output.push_str("=== Conversation Transcript ===\n");
    let _ = writeln!(output, "Session: {}", state.session.id);
    let _ = writeln!(output, "Model: {}", state.session.model);
    let _ = write!(output, "Messages: {}\n\n", state.messages.len());

    for (idx, msg) in state.messages.iter().enumerate() {
        match &msg.role {
            MessageRole::User => {
                let _ = writeln!(output, "--- User #{} ---", idx + 1);
                output.push_str(&strip_ansi(&msg.full_text()));
                output.push_str("\n\n");
            }
            MessageRole::Assistant => {
                let _ = writeln!(output, "--- {} #{} ---", msg.agent, idx + 1);
                for part in &msg.parts {
                    match part {
                        MessagePart::Text { content } => {
                            output.push_str(&strip_ansi(content));
                            output.push_str("\n\n");
                        }
                        MessagePart::Thinking { content } => {
                            output.push_str("[thinking]\n");
                            output.push_str(&strip_ansi(content));
                            output.push_str("\n\n");
                        }
                        MessagePart::ToolCall {
                            name,
                            input_summary,
                            output: tool_output,
                            status,
                            ..
                        } => {
                            let status_label = match status {
                                ToolStatus::Completed => "completed",
                                ToolStatus::Failed => "failed",
                                ToolStatus::Running => "running",
                                ToolStatus::Pending => "pending",
                            };
                            let _ = writeln!(output, "  [tool: {name} ({status_label})]");
                            let _ = writeln!(output, "  Input: {}", strip_ansi(input_summary));
                            if let Some(out) = tool_output {
                                let _ = writeln!(output, "  Output: {}", strip_ansi(out));
                            }
                            output.push('\n');
                        }
                    }
                }
            }
            MessageRole::Tool { name } => {
                let _ = writeln!(output, "--- Tool: {name} ---");
                if let Some(tool) = state.tools.iter().rev().find(|t| &t.name == name) {
                    let _ = writeln!(output, "  Status: {:?}", tool.status);
                    let _ = writeln!(output, "  Input: {}", strip_ansi(&tool.input_summary));
                }
                output.push('\n');
            }
        }
    }

    output.push_str("--- Session Stats ---\n");
    let _ = writeln!(output, "Turns: {}", state.session.turns);
    let _ = writeln!(output, "Input tokens: {}", state.session.input_tokens);
    let _ = writeln!(output, "Output tokens: {}", state.session.output_tokens);
    let _ = writeln!(output, "Cost: ${:.4}", state.session.cumulative_cost);

    Ok(output)
}

fn format_markdown(state: &AppState) -> Result<String, String> {
    let mut output = String::new();

    output.push_str("# Conversation Transcript\n\n");
    let _ = writeln!(output, "**Session**: {}", state.session.id);
    let _ = writeln!(output, "**Model**: {}", state.session.model);
    let _ = write!(output, "**Messages**: {}\n\n", state.messages.len());
    output.push_str("---\n\n");

    for (idx, msg) in state.messages.iter().enumerate() {
        match &msg.role {
            MessageRole::User => {
                let _ = write!(output, "### User #{}\n\n", idx + 1);
                output.push_str(&msg.full_text());
                output.push_str("\n\n");
            }
            MessageRole::Assistant => {
                let _ = write!(output, "### {} #{}\n\n", msg.agent, idx + 1);
                for part in &msg.parts {
                    match part {
                        MessagePart::Text { content } => {
                            output.push_str(content);
                            output.push_str("\n\n");
                        }
                        MessagePart::Thinking { content } => {
                            output.push_str("> *Thinking:*\n> ");
                            output.push_str(&content.replace('\n', "\n> "));
                            output.push_str("\n\n");
                        }
                        MessagePart::ToolCall {
                            name,
                            input_summary,
                            output: tool_output,
                            status,
                            ..
                        } => {
                            let status_emoji = match status {
                                ToolStatus::Completed => "✅",
                                ToolStatus::Failed => "❌",
                                ToolStatus::Running => "⏳",
                                ToolStatus::Pending => "⏸️",
                            };
                            let _ = write!(output, "**Tool**: {name} {status_emoji}\n\n");

                            if let Ok(parsed) =
                                serde_json::from_str::<serde_json::Value>(input_summary)
                            {
                                let _ = write!(
                                    output,
                                    "```json\n{}\n```\n\n",
                                    serde_json::to_string_pretty(&parsed)
                                        .unwrap_or_else(|_| input_summary.clone())
                                );
                            } else {
                                let _ = write!(output, "```json\n{input_summary}\n```\n\n");
                            }

                            if let Some(out) = tool_output {
                                output.push_str("**Output:**\n\n");
                                if out.lines().count() > 20 {
                                    let preview: String =
                                        out.lines().take(20).collect::<Vec<_>>().join("\n");
                                    let _ = write!(
                                        output,
                                        "```\n{}\n...\n(+ {} more lines)\n```\n\n",
                                        preview,
                                        out.lines().count() - 20
                                    );
                                } else {
                                    let _ = write!(output, "```\n{out}\n```\n\n");
                                }
                            }
                        }
                    }
                }
            }
            MessageRole::Tool { name } => {
                let _ = write!(output, "### Tool Result: {name}\n\n");
                if let Some(tool) = state.tools.iter().rev().find(|t| &t.name == name) {
                    let status_emoji = match tool.status {
                        ToolStatus::Completed => "✅",
                        ToolStatus::Failed => "❌",
                        ToolStatus::Running => "⏳",
                        ToolStatus::Pending => "⏸️",
                    };
                    let _ = write!(output, "Status: {} {:?}\n\n", status_emoji, tool.status);
                    let _ = write!(output, "Input: `{}`\n\n", strip_ansi(&tool.input_summary));
                }
            }
        }
    }

    output.push_str("---\n\n");
    output.push_str("## Session Stats\n\n");
    let _ = writeln!(output, "- **Turns**: {}", state.session.turns);
    let _ = writeln!(output, "- **Input tokens**: {}", state.session.input_tokens);
    let _ = writeln!(
        output,
        "- **Output tokens**: {}",
        state.session.output_tokens
    );
    let _ = writeln!(output, "- **Cost**: ${:.4}", state.session.cumulative_cost);

    Ok(output)
}

fn format_json(state: &AppState) -> Result<String, String> {
    let messages: Vec<serde_json::Value> = state
        .messages
        .iter()
        .map(|msg| {
            let role_str = match &msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool { name } => "tool",
            };

            let parts: Vec<serde_json::Value> = msg
                .parts
                .iter()
                .map(|part| match part {
                    MessagePart::Text { content } => serde_json::json!({
                        "type": "text",
                        "content": content,
                    }),
                    MessagePart::Thinking { content } => serde_json::json!({
                        "type": "thinking",
                        "content": content,
                    }),
                    MessagePart::ToolCall {
                        id,
                        name,
                        status,
                        input_summary,
                        output,
                        ..
                    } => {
                        let status_str = match status {
                            ToolStatus::Completed => "completed",
                            ToolStatus::Failed => "failed",
                            ToolStatus::Running => "running",
                            ToolStatus::Pending => "pending",
                        };
                        let mut obj = serde_json::json!({
                            "type": "tool_call",
                            "id": id,
                            "name": name,
                            "status": status_str,
                            "input": input_summary,
                        });
                        if let Some(out) = output {
                            obj["output"] = serde_json::Value::String(out.clone());
                        }
                        obj
                    }
                })
                .collect();

            serde_json::json!({
                "role": role_str,
                "agent": msg.agent,
                "timestamp": msg.timestamp,
                "is_streaming": msg.is_streaming,
                "parts": parts,
            })
        })
        .collect();

    let export = serde_json::json!({
        "session_id": state.session.id,
        "model": state.session.model,
        "permission_mode": state.session.permission_mode,
        "turns": state.session.turns,
        "input_tokens": state.session.input_tokens,
        "output_tokens": state.session.output_tokens,
        "cache_create_tokens": state.session.cache_create_tokens,
        "cache_read_tokens": state.session.cache_read_tokens,
        "cumulative_cost": state.session.cumulative_cost,
        "budget_max": state.session.budget_max,
        "budget_remaining": state.session.budget_remaining,
        "compaction_count": state.session.compaction_count,
        "messages": messages,
    });

    serde_json::to_string_pretty(&export).map_err(|e| format!("JSON serialization error: {e}"))
}

fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_escape = false;

    for c in input.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' || c.is_ascii_uppercase() || c.is_ascii_lowercase() {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let mut state = AppState::new("sonnet", "workspace-write", "/tmp", None);
        state.session.id = "test-123".to_string();
        state
    }

    #[test]
    fn test_strip_ansi_removes_codes() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1b[1;32mbold green\x1b[0m"), "bold green");
        assert_eq!(strip_ansi("no codes"), "no codes");
        assert_eq!(strip_ansi("\x1b[38;5;200mcustom\x1b[0m"), "custom");
    }

    #[test]
    fn test_format_plain_basic() {
        let mut state = test_state();
        state.add_user_message("hello world".to_string());
        state.start_assistant_stream("build");
        state.append_to_stream("Hi there!");
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Plain);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("=== Conversation Transcript ==="));
        assert!(output.contains("Session: test-123"));
        assert!(output.contains("--- User #1 ---"));
        assert!(output.contains("hello world"));
        assert!(output.contains("--- build #2 ---"));
        assert!(output.contains("Hi there!"));
    }

    #[test]
    fn test_format_plain_strips_ansi() {
        let mut state = test_state();
        state.add_user_message("\x1b[31mcolored\x1b[0m text".to_string());

        let result = export_transcript(&state, TranscriptFormat::Plain);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("colored text"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_format_markdown_basic() {
        let mut state = test_state();
        state.add_user_message("test prompt".to_string());
        state.start_assistant_stream("build");
        state.append_to_stream("Response text");
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Markdown);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("# Conversation Transcript"));
        assert!(output.contains("### User #1"));
        assert!(output.contains("test prompt"));
        assert!(output.contains("### build #2"));
        assert!(output.contains("Response text"));
    }

    #[test]
    fn test_format_markdown_thinking() {
        let mut state = test_state();
        state.start_assistant_stream("build");
        state.start_thinking();
        state.append_thinking("Let me think...");
        state.end_thinking();
        state.append_to_stream("Answer");
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Markdown);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("*Thinking:*"));
        assert!(output.contains("Let me think..."));
    }

    #[test]
    fn test_format_json_structure() {
        let mut state = test_state();
        state.add_user_message("hello".to_string());
        state.start_assistant_stream("build");
        state.append_to_stream("world");
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Json);
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["session_id"], "test-123");
        assert_eq!(parsed["model"], "sonnet");
        assert!(parsed["messages"].is_array());
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["messages"][0]["role"], "user");
        assert_eq!(parsed["messages"][1]["role"], "assistant");
    }

    #[test]
    fn test_format_json_tool_calls() {
        let mut state = test_state();
        state.start_assistant_stream("build");
        state.add_tool_event("bash", "{\"command\": \"ls\"}");
        state.complete_tool_event("bash", "file1.txt\nfile2.txt", true);
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Json);
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        let assistant = &parsed["messages"][0];
        assert_eq!(assistant["role"], "assistant");
        let parts = assistant["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[0]["name"], "bash");
        assert_eq!(parts[0]["status"], "completed");
    }

    #[test]
    fn test_format_json_tool_calls_failed() {
        let mut state = test_state();
        state.start_assistant_stream("build");
        state.add_tool_event("bash", "{\"command\": \"rm -rf /\"}");
        state.complete_tool_event("bash", "permission denied", false);
        state.finish_stream();

        let result = export_transcript(&state, TranscriptFormat::Json);
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        let assistant = &parsed["messages"][0];
        let parts = assistant["parts"].as_array().unwrap();
        assert_eq!(parts[0]["status"], "failed");
    }

    #[test]
    fn test_export_empty_session() {
        let state = test_state();

        let plain = export_transcript(&state, TranscriptFormat::Plain);
        assert!(plain.is_ok());

        let md = export_transcript(&state, TranscriptFormat::Markdown);
        assert!(md.is_ok());

        let json = export_transcript(&state, TranscriptFormat::Json);
        assert!(json.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn test_strip_ansi_multiple_codes() {
        assert_eq!(
            strip_ansi("\x1b[1m\x1b[31mbold red\x1b[0m\x1b[0m"),
            "bold red"
        );
    }
}
