use crate::tui::app::{AppState, MessagePart, MessageRole, TextSelection, ToolStatus};
use crate::tui::markdown::render_markdown_to_lines;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::prelude::StatefulWidget;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

const MAX_EXPANDED_LINES: usize = 10;
const INLINE_LABEL_MAX: usize = 60;

pub struct MessageList;

enum RenderItem {
    Separator,
    RevertNotice(Vec<Span<'static>>),
    TextLines(Vec<Line<'static>>),
    ToolCallInline(ToolCallData),
    ToolCallBlock(ToolCallData),
    TodoList(Vec<TodoItemData>),
    Thinking(Vec<Line<'static>>),
    ThinkingPlaceholder,
    Cursor(Color),
    AgentSignature {
        agent: String,
        model: String,
        color: Color,
    },
}

#[derive(Clone)]
struct ToolCallData {
    name: String,
    status: ToolStatus,
    input_summary: String,
    output: Option<String>,
    expanded: bool,
    timestamp: u64,
}

#[derive(Clone)]
struct TodoItemData {
    content: String,
    status: TodoStatus,
}

#[derive(Clone, Copy, PartialEq)]
enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

fn tool_icon(name: &str) -> &'static str {
    match name {
        "bash" | "sh" => "$",
        "read" | "cat" | "read_file" => "→",
        "write" | "create" | "save" | "write_file" => "←",
        "edit" | "patch" | "replace" | "edit_file" => "✎",
        "glob" | "find" | "glob_search" => "✱",
        "grep" | "search" | "grep_search" => "✱",
        "web_search" => "◇",
        "web_fetch" | "fetch" => "%",
        "task" | "delegate" => "│",
        "todo_write" | "todo" => "☑",
        "notebook_edit" => "N",
        _ => "⚙",
    }
}

fn human_tool_title(name: &str, input_summary: &str) -> String {
    let label = human_tool_label(name, input_summary);
    match name {
        "bash" | "sh" => "Shell".to_string(),
        "read" | "cat" | "read_file" => {
            if label.is_empty() {
                "Read".to_string()
            } else {
                format!("Read {label}")
            }
        }
        "write" | "create" | "save" | "write_file" => {
            if label.is_empty() {
                "Wrote".to_string()
            } else {
                format!("Wrote {label}")
            }
        }
        "edit" | "patch" | "replace" | "edit_file" => {
            if label.is_empty() {
                "Edited".to_string()
            } else {
                format!("Edited {label}")
            }
        }
        "glob" | "find" | "glob_search" => {
            if label.is_empty() {
                "Glob".to_string()
            } else {
                format!("Glob {label}")
            }
        }
        "grep" | "search" | "grep_search" => {
            if label.is_empty() {
                "Grep".to_string()
            } else {
                format!("Grep {label}")
            }
        }
        "web_search" => {
            if label.is_empty() {
                "Web Search".to_string()
            } else {
                format!("Search {label}")
            }
        }
        "web_fetch" | "fetch" => {
            if label.is_empty() {
                "Fetch".to_string()
            } else {
                format!("Fetch {label}")
            }
        }
        "task" | "delegate" => "Task".to_string(),
        "todo_write" | "todo" => "Todo".to_string(),
        "notebook_edit" => "Notebook".to_string(),
        _ => {
            let display_name = capitalize_first(name);
            if label.is_empty() {
                display_name
            } else {
                format!("{display_name} {label}")
            }
        }
    }
}

fn human_tool_label(name: &str, input_json: &str) -> String {
    if input_json.is_empty() {
        return String::new();
    }

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input_json) {
        let extracted = extract_json_label(&val, name);
        if !extracted.is_empty() {
            return truncate_label(&extracted, INLINE_LABEL_MAX);
        }
    }

    truncate_label(input_json, INLINE_LABEL_MAX)
}

fn extract_json_label(val: &serde_json::Value, tool_name: &str) -> String {
    if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
        if !desc.is_empty() {
            return desc.to_string();
        }
    }

    match tool_name {
        "bash" | "sh" => val
            .get("command")
            .or_else(|| val.get("cmd"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "read" | "cat" | "read_file" | "write" | "create" | "save" | "write_file" | "edit"
        | "patch" | "replace" | "edit_file" => val
            .get("filePath")
            .or_else(|| val.get("path"))
            .or_else(|| val.get("file"))
            .and_then(|v| v.as_str())
            .map(strip_path_prefix)
            .unwrap_or_default(),
        "glob" | "find" | "glob_search" => val
            .get("pattern")
            .or_else(|| val.get("glob"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "grep" | "search" | "grep_search" => val
            .get("pattern")
            .or_else(|| val.get("regex"))
            .or_else(|| val.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "web_search" => val
            .get("query")
            .or_else(|| val.get("search"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "web_fetch" | "fetch" => val
            .get("url")
            .or_else(|| val.get("uri"))
            .and_then(|v| v.as_str())
            .map(strip_url_prefix)
            .unwrap_or_default(),
        "task" | "delegate" => val
            .get("description")
            .or_else(|| val.get("task"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "todo_write" | "todo" => val
            .get("todos")
            .and_then(|v| v.as_array())
            .map(|arr| {
                let items: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.get("content").and_then(|v| v.as_str()))
                    .take(3)
                    .map(|s| s.to_string())
                    .collect();
                items.join(", ")
            })
            .unwrap_or_default(),
        "notebook_edit" => val
            .get("path")
            .or_else(|| val.get("filePath"))
            .or_else(|| val.get("notebook"))
            .and_then(|v| v.as_str())
            .map(strip_path_prefix)
            .unwrap_or_default(),
        _ => {
            for key in [
                "filePath", "path", "command", "pattern", "query", "url", "file", "name",
            ] {
                if let Some(s) = val.get(key).and_then(|v| v.as_str()) {
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
            String::new()
        }
    }
}

fn strip_path_prefix(path: &str) -> String {
    let path = path.strip_prefix("./").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 3 {
        format!("…/{}", parts[parts.len().saturating_sub(2)..].join("/"))
    } else {
        path.to_string()
    }
}

fn strip_url_prefix(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .to_string()
}

fn truncate_label(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else {
        let mut end = max_width.saturating_sub(1);
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

fn extract_bash_command(input_summary: &str) -> String {
    if input_summary.is_empty() {
        return String::new();
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input_summary) {
        if let Some(cmd) = val
            .get("command")
            .or_else(|| val.get("cmd"))
            .and_then(|v| v.as_str())
        {
            let truncated = truncate_label(cmd, INLINE_LABEL_MAX);
            return truncated;
        }
    }
    String::new()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn parse_todos_from_input(input_json: &str) -> Vec<TodoItemData> {
    if input_json.is_empty() {
        return Vec::new();
    }
    let Ok(val) = serde_json::from_str::<serde_json::Value>(input_json) else {
        return Vec::new();
    };
    let Some(arr) = val.get("todos").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|t| {
            let content = t.get("content").and_then(|v| v.as_str())?.to_string();
            let status = t
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "completed" => TodoStatus::Completed,
                    "in_progress" => TodoStatus::InProgress,
                    _ => TodoStatus::Pending,
                })
                .unwrap_or(TodoStatus::Pending);
            Some(TodoItemData { content, status })
        })
        .collect()
}

impl MessageList {
    pub const fn new() -> Self {
        Self
    }

    pub fn render(frame: &mut Frame, state: &AppState, area: Rect) -> usize {
        let messages = &state.messages;
        let revert_boundary = state.revert.as_ref().map(|r| r.message_boundary);

        if messages.is_empty() {
            let empty = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Start a conversation",
                    Style::default()
                        .fg(state.theme.text_muted)
                        .add_modifier(Modifier::ITALIC),
                )),
                Line::from(Span::styled(
                    "  Type your prompt below and press Enter",
                    Style::default()
                        .fg(state.theme.text_muted)
                        .add_modifier(Modifier::ITALIC),
                )),
            ]);
            frame.render_widget(empty, area);
            return 0;
        }

        let content_width = area.width.saturating_sub(2) as usize;
        let mut items: Vec<RenderItem> = Vec::new();
        let mut line_counts: Vec<usize> = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    continue;
                }
                if idx == boundary.saturating_sub(1) {
                    items.push(RenderItem::Separator);
                    line_counts.push(1);
                    let revert_spans = vec![
                        Span::styled(
                            " ↩ ",
                            Style::default()
                                .fg(state.theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{} message(s) reverted", state.reverted_count()),
                            Style::default()
                                .fg(state.theme.warning)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("  •  ", Style::default().fg(state.theme.text_muted)),
                        Span::styled(
                            "↻ ",
                            Style::default()
                                .fg(state.theme.primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "PgDn to redo",
                            Style::default()
                                .fg(state.theme.primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ];
                    items.push(RenderItem::RevertNotice(revert_spans));
                    line_counts.push(1);
                    items.push(RenderItem::Separator);
                    line_counts.push(1);
                }
            }

            let agent_color = state.theme.agent_color(&msg.agent);

            match &msg.role {
                MessageRole::User => {
                    if !items.is_empty() {
                        items.push(RenderItem::Separator);
                        line_counts.push(1);
                        items.push(RenderItem::TextLines(vec![Line::from(vec![Span::styled(
                            "─".repeat(content_width.min(80)),
                            Style::default().fg(state.theme.border),
                        )])]));
                        line_counts.push(1);
                        items.push(RenderItem::Separator);
                        line_counts.push(1);
                    }
                    let text = msg.full_text();
                    let wrapped = wrap_text(&text, content_width.saturating_sub(3));
                    let mut user_lines = Vec::new();
                    for (i, line_text) in wrapped.into_iter().enumerate() {
                        let prefix = if i == 0 {
                            Span::styled(
                                "│",
                                Style::default()
                                    .fg(agent_color)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::raw(" ")
                        };
                        user_lines.push(Line::from(vec![
                            prefix,
                            Span::raw(" "),
                            Span::styled(line_text, Style::default().fg(state.theme.text)),
                        ]));
                    }
                    if !user_lines.is_empty() {
                        let lc = user_lines.len();
                        items.push(RenderItem::TextLines(user_lines));
                        line_counts.push(lc);
                    }
                }
                MessageRole::Assistant => {
                    if !items.is_empty() {
                        items.push(RenderItem::Separator);
                        line_counts.push(1);
                    }
                    let full_text = msg.full_text();
                    let has_text = !full_text.is_empty();

                    for part in &msg.parts {
                        match part {
                            MessagePart::Text { content } => {
                                let md_lines = render_markdown_to_lines(
                                    content,
                                    content_width.saturating_sub(4),
                                    &state.theme,
                                );
                                let mut prefixed = Vec::new();
                                for md_line in md_lines {
                                    let mut spans = Vec::with_capacity(md_line.spans.len() + 1);
                                    spans.push(Span::raw("  "));
                                    spans.extend(md_line.spans);
                                    prefixed.push(Line::from(spans));
                                }
                                let lc = prefixed.len();
                                items.push(RenderItem::TextLines(prefixed));
                                line_counts.push(lc);
                            }
                            MessagePart::Thinking { content } if state.show_thinking => {
                                let thinking_lines = build_thinking_lines(
                                    content,
                                    content_width.saturating_sub(4),
                                    &state.theme,
                                );
                                let lc = thinking_lines.len();
                                items.push(RenderItem::Thinking(thinking_lines));
                                line_counts.push(lc);
                            }
                            MessagePart::Thinking { .. } => {}
                            MessagePart::ToolCall {
                                name,
                                status,
                                input_summary,
                                output,
                                expanded,
                                ..
                            } => {
                                let data = ToolCallData {
                                    name: name.clone(),
                                    status: *status,
                                    input_summary: input_summary.clone(),
                                    output: output.clone(),
                                    expanded: *expanded,
                                    timestamp: msg.timestamp,
                                };
                                let has_output = data.output.is_some()
                                    && !data.output.as_ref().map_or(true, |s| s.is_empty());
                                match data.status {
                                    ToolStatus::Pending | ToolStatus::Running => {
                                        items.push(RenderItem::ToolCallInline(data));
                                        line_counts.push(1);
                                    }
                                    ToolStatus::Completed | ToolStatus::Failed => {
                                        if data.name == "todo_write" || data.name == "todo" {
                                            let todos = parse_todos_from_input(&data.input_summary);
                                            if !todos.is_empty() {
                                                let h = todos.len() + 2;
                                                items.push(RenderItem::TodoList(todos));
                                                line_counts.push(h);
                                            } else {
                                                items.push(RenderItem::ToolCallInline(data));
                                                line_counts.push(1);
                                            }
                                        } else if has_output {
                                            let block_h = compute_tool_call_block_height(
                                                &data,
                                                content_width,
                                                &state.theme,
                                            );
                                            items.push(RenderItem::ToolCallBlock(data));
                                            line_counts.push(block_h);
                                        } else {
                                            items.push(RenderItem::ToolCallInline(data));
                                            line_counts.push(1);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if msg.is_streaming {
                        if !has_text && state.show_thinking && state.is_thinking {
                            items.push(RenderItem::ThinkingPlaceholder);
                            line_counts.push(1);
                        } else if !has_text {
                        } else {
                            items.push(RenderItem::Cursor(agent_color));
                            line_counts.push(1);
                        }
                    } else if has_text {
                        items.push(RenderItem::AgentSignature {
                            agent: msg.agent.clone(),
                            model: state.session.model.clone(),
                            color: agent_color,
                        });
                        line_counts.push(1);
                    }
                }
                MessageRole::Tool { name } => {
                    if !items.is_empty() {
                        items.push(RenderItem::Separator);
                        line_counts.push(1);
                        items.push(RenderItem::TextLines(vec![Line::from(vec![Span::styled(
                            "─".repeat(content_width.min(80)),
                            Style::default().fg(state.theme.border),
                        )])]));
                        line_counts.push(1);
                        items.push(RenderItem::Separator);
                        line_counts.push(1);
                    }
                    let tool = state.tools.iter().rev().find(|t| t.name == *name);
                    let status = tool.map(|t| t.status).unwrap_or(ToolStatus::Completed);
                    let (icon, color) = match status {
                        ToolStatus::Pending | ToolStatus::Running => ("○", state.theme.warning),
                        ToolStatus::Completed => ("✓", state.theme.success),
                        ToolStatus::Failed => ("✗", state.theme.error),
                    };
                    let mut tool_lines = vec![Line::from(vec![
                        Span::raw("  "),
                        Span::styled(icon, Style::default().fg(color)),
                        Span::raw(" "),
                        Span::styled(name.clone(), Style::default().fg(state.theme.text_muted)),
                    ])];
                    if let Some(t) = tool {
                        if !t.input_summary.is_empty() {
                            let summary_lines =
                                wrap_text(&t.input_summary, content_width.saturating_sub(6));
                            for s in summary_lines {
                                tool_lines.push(Line::from(vec![
                                    Span::raw("     "),
                                    Span::styled(s, Style::default().fg(state.theme.text_muted)),
                                ]));
                            }
                        }
                    }
                    let lc = tool_lines.len();
                    items.push(RenderItem::TextLines(tool_lines));
                    line_counts.push(lc);
                }
            }
        }

        let total_lines: usize = line_counts.iter().sum();
        let visible_lines = area.height as usize;

        if visible_lines == 0 || total_lines == 0 {
            return total_lines;
        }

        let scroll = if state.scroll_offset == usize::MAX {
            total_lines.saturating_sub(visible_lines)
        } else {
            state
                .scroll_offset
                .min(total_lines.saturating_sub(visible_lines))
        };

        let start = scroll;
        let end = start + visible_lines;

        let mut current_line = 0;

        for item in &items {
            let item_height = match item {
                RenderItem::Separator => 1,
                RenderItem::RevertNotice(_) => 1,
                RenderItem::TextLines(ls) => ls.len(),
                RenderItem::ToolCallInline(_) => 1,
                RenderItem::ToolCallBlock(data) => {
                    compute_tool_call_block_height(data, content_width, &state.theme)
                }
                RenderItem::TodoList(todos) => todos.len() + 2,
                RenderItem::Thinking(lines) => lines.len(),
                RenderItem::ThinkingPlaceholder => 1,
                RenderItem::Cursor(_) => 1,
                RenderItem::AgentSignature { .. } => 1,
            };

            let item_start = current_line;
            let item_end = current_line + item_height;

            if item_end <= start {
                current_line = item_end;
                continue;
            }
            if item_start >= end {
                break;
            }

            let visible_start = start.saturating_sub(item_start);
            let visible_end = end.min(item_end);
            let visible_count = visible_end.saturating_sub(visible_start);

            if visible_count == 0 {
                current_line = item_end;
                continue;
            }

            let item_y = area.y + (item_start.saturating_sub(start)) as u16;
            let item_area = Rect {
                x: area.x,
                y: item_y,
                width: area.width,
                height: visible_count as u16,
            };

            match item {
                RenderItem::Separator => {
                    frame.render_widget(Paragraph::new(Line::from("")), item_area);
                }
                RenderItem::RevertNotice(spans) => {
                    frame.render_widget(Paragraph::new(Line::from(spans.clone())), item_area);
                }
                RenderItem::TextLines(ls) => {
                    let visible: Vec<Line<'_>> = ls[visible_start..visible_end.min(ls.len())]
                        .iter()
                        .cloned()
                        .collect();
                    frame.render_widget(Paragraph::new(visible), item_area);
                }
                RenderItem::ToolCallInline(data) => {
                    render_tool_call_inline(
                        frame,
                        data,
                        item_area,
                        &state.theme,
                        visible_start,
                        visible_end,
                    );
                }
                RenderItem::ToolCallBlock(data) => {
                    render_tool_call_block(
                        frame,
                        data,
                        item_area,
                        content_width,
                        &state.theme,
                        visible_start,
                        visible_end,
                    );
                }
                RenderItem::TodoList(todos) => {
                    render_todo_list(
                        frame,
                        todos,
                        item_area,
                        &state.theme,
                        visible_start,
                        visible_end,
                    );
                }
                RenderItem::Thinking(lines) => {
                    let visible: Vec<Line<'_>> = lines[visible_start..visible_end.min(lines.len())]
                        .iter()
                        .cloned()
                        .collect();
                    let block = Block::new()
                        .borders(Borders::LEFT)
                        .border_type(BorderType::Double)
                        .border_style(Style::default().fg(state.theme.text_muted))
                        .style(Style::default().bg(state.theme.background_element))
                        .padding(Padding::new(0, 0, 0, 1));
                    frame.render_widget(Paragraph::new(visible).block(block), item_area);
                }
                RenderItem::ThinkingPlaceholder => {
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                "▸ Thinking  ···",
                                Style::default()
                                    .fg(Color::Rgb(157, 124, 216))
                                    .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                            ),
                        ])),
                        item_area,
                    );
                }
                RenderItem::Cursor(color) => {
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                "█",
                                Style::default()
                                    .fg(*color)
                                    .add_modifier(Modifier::RAPID_BLINK),
                            ),
                        ])),
                        item_area,
                    );
                }
                RenderItem::AgentSignature {
                    agent,
                    model,
                    color,
                } => {
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled("▣ ", Style::default().fg(*color)),
                            Span::styled(
                                agent.clone(),
                                Style::default().fg(*color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" · {}", model),
                                Style::default().fg(state.theme.text_muted),
                            ),
                        ])),
                        item_area,
                    );
                }
            }

            current_line = item_end;
        }

        if total_lines > visible_lines {
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some(" "))
                .thumb_symbol("█")
                .style(Style::default().fg(state.theme.text_muted))
                .render(
                    area,
                    frame.buffer_mut(),
                    &mut ScrollbarState::new(total_lines)
                        .position(scroll)
                        .viewport_content_length(visible_lines),
                );
        }

        total_lines
    }
}

fn compute_tool_call_block_height(
    data: &ToolCallData,
    content_width: usize,
    _theme: &Theme,
) -> usize {
    let mut h = 1;
    if data.expanded {
        let cmd = extract_bash_command(&data.input_summary);
        if !cmd.is_empty() {
            h += 1;
        }
        if let Some(ref out) = data.output {
            let out_lines = wrap_text(out, content_width.saturating_sub(4));
            let max_out = MAX_EXPANDED_LINES.min(out_lines.len());
            h += max_out;
            if out_lines.len() > max_out {
                h += 1;
            }
        }
    }
    h += 1;
    h
}

fn render_tool_call_inline(
    frame: &mut Frame,
    data: &ToolCallData,
    area: Rect,
    theme: &Theme,
    skip: usize,
    take: usize,
) {
    if skip > 0 || take == 0 {
        return;
    }

    let spans = match data.status {
        ToolStatus::Pending | ToolStatus::Running => {
            let cmd = extract_bash_command(&data.input_summary);
            let icon = tool_icon(&data.name);
            let pending_label = human_tool_title(&data.name, &data.input_summary);
            if !cmd.is_empty() {
                vec![
                    Span::raw("   "),
                    Span::styled(
                        format!("{icon} "),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(cmd, Style::default().fg(theme.text)),
                ]
            } else {
                vec![
                    Span::raw("   "),
                    Span::styled(
                        format!("{icon} "),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        pending_label,
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]
            }
        }
        ToolStatus::Completed => {
            let label = human_tool_title(&data.name, &data.input_summary);
            vec![
                Span::raw("   "),
                Span::styled(format!("✓ {label}"), Style::default().fg(theme.success)),
            ]
        }
        ToolStatus::Failed => {
            let label = human_tool_title(&data.name, &data.input_summary);
            vec![
                Span::raw("   "),
                Span::styled(format!("✗ {label}"), Style::default().fg(theme.error)),
            ]
        }
    };

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_tool_call_block(
    frame: &mut Frame,
    data: &ToolCallData,
    area: Rect,
    content_width: usize,
    theme: &Theme,
    skip: usize,
    take: usize,
) {
    let title = human_tool_title(&data.name, &data.input_summary);

    let mut all_lines: Vec<Line<'static>> = Vec::new();

    let title_style = Style::default()
        .fg(theme.text_muted)
        .add_modifier(Modifier::BOLD);
    all_lines.push(Line::from(vec![Span::styled(
        format!("# {title}"),
        title_style,
    )]));

    if data.expanded {
        let cmd = extract_bash_command(&data.input_summary);
        if !cmd.is_empty() {
            all_lines.push(Line::from(vec![Span::styled(
                format!("$ {cmd}"),
                Style::default().fg(theme.text_muted),
            )]));
        }

        if let Some(ref out) = data.output {
            let out_lines = wrap_text(out, content_width.saturating_sub(4));
            let max_out = MAX_EXPANDED_LINES.min(out_lines.len());
            for out_line in out_lines.iter().take(max_out) {
                all_lines.push(Line::from(vec![Span::styled(
                    out_line.clone(),
                    Style::default().fg(theme.text_muted),
                )]));
            }
            if out_lines.len() > max_out {
                all_lines.push(Line::from(vec![Span::styled(
                    format!("... {} more lines", out_lines.len() - max_out),
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::ITALIC),
                )]));
            }
        }
    } else if let Some(ref out) = data.output {
        let out_lines = wrap_text(out, content_width.saturating_sub(4));
        let preview_lines = 2.min(out_lines.len());
        for out_line in out_lines.iter().take(preview_lines) {
            all_lines.push(Line::from(vec![Span::styled(
                out_line.clone(),
                Style::default().fg(theme.text_muted),
            )]));
        }
        if out_lines.len() > preview_lines {
            all_lines.push(Line::from(vec![Span::styled(
                "... expand for more",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )]));
        }
    }

    let hint_text = if data.expanded {
        "click to collapse"
    } else {
        "click to expand"
    };
    all_lines.push(Line::from(vec![Span::styled(
        hint_text,
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    )]));

    let start = skip.min(all_lines.len());
    let end = (start + take).min(all_lines.len());
    if start >= end {
        return;
    }

    let visible: Vec<Line<'_>> = all_lines[start..end].iter().cloned().collect();

    let bg = if data.expanded {
        theme.background_element
    } else {
        theme.background_panel
    };

    let block = Block::new()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(bg))
        .padding(Padding::new(2, 0, 1, 0));

    let paragraph = Paragraph::new(visible).block(block);
    frame.render_widget(paragraph, area);
}

fn render_todo_list(
    frame: &mut Frame,
    todos: &[TodoItemData],
    area: Rect,
    theme: &Theme,
    skip: usize,
    take: usize,
) {
    let mut all_lines: Vec<Line<'static>> = Vec::new();

    all_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Todos",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]));

    for todo in todos {
        let (indicator, color) = match todo.status {
            TodoStatus::Completed => ("[✓]", theme.success),
            TodoStatus::InProgress => ("[•]", theme.warning),
            TodoStatus::Pending => ("[ ]", theme.text_muted),
        };
        all_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{indicator} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(todo.content.clone(), Style::default().fg(theme.text)),
        ]));
    }

    let start = skip.min(all_lines.len());
    let end = (start + take).min(all_lines.len());
    if start >= end {
        return;
    }

    let visible: Vec<Line<'_>> = all_lines[start..end].iter().cloned().collect();

    let block = Block::new()
        .borders(Borders::LEFT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.success))
        .style(Style::default().bg(theme.background_element))
        .padding(Padding::new(0, 0, 0, 0));

    let paragraph = Paragraph::new(visible).block(block);
    frame.render_widget(paragraph, area);
}

fn build_thinking_lines(content: &str, content_width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let thinking_style = Style::default()
        .fg(Color::Rgb(157, 124, 216))
        .add_modifier(Modifier::ITALIC);
    let label_style = Style::default()
        .fg(Color::Rgb(157, 124, 216))
        .add_modifier(Modifier::BOLD | Modifier::ITALIC);

    if content.is_empty() {
        return vec![Line::from(vec![
            Span::raw(" "),
            Span::styled("Thinking", label_style),
            Span::styled("  ···", thinking_style),
        ])];
    }

    let wrapped = wrap_text(content, content_width.saturating_sub(6));
    let mut lines = Vec::new();

    for (i, line_text) in wrapped.into_iter().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled("  ▸ ", label_style),
                Span::styled(line_text, thinking_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(line_text, thinking_style),
            ]));
        }
    }

    lines
}

pub fn wrap_text(text: &str, max_display_width: usize) -> Vec<String> {
    if max_display_width == 0 {
        return vec![text.into()];
    }
    let mut result = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut chunk_start = 0;
        let mut current_width = 0;
        for (byte_idx, ch) in line.char_indices() {
            let char_w = ch.width().unwrap_or(1);
            if current_width + char_w > max_display_width {
                result.push(line[chunk_start..byte_idx].to_string());
                chunk_start = byte_idx;
                current_width = char_w;
            } else {
                current_width += char_w;
            }
        }
        if chunk_start < line.len() {
            result.push(line[chunk_start..].to_string());
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

pub fn render_selection_highlight(
    buf: &mut ratatui::buffer::Buffer,
    selection: &TextSelection,
    area: Rect,
    theme: &Theme,
) {
    let min_row = selection.start_row.min(selection.end_row);
    let max_row = selection.start_row.max(selection.end_row);

    for row in min_row..=max_row {
        if row < area.y || row >= area.bottom() {
            continue;
        }
        for col in area.x..area.right() {
            if let Some(cell) = buf.cell((col, row)) {
                let is_empty = cell.symbol().is_empty() || cell.symbol() == " ";
                if !is_empty {
                    if let Some(cell) = buf.cell_mut((col, row)) {
                        cell.set_fg(theme.text_inverse).set_bg(theme.primary);
                    }
                }
            }
        }
    }
}
