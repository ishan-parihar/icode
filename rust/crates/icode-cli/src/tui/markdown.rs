use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthChar;

use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
}

impl InlineStyle {
    fn to_ratatui(&self, theme: &Theme) -> Style {
        let mut style = Style::default().fg(theme.text);
        if self.code {
            style = style.fg(theme.code_text).bg(theme.code_bg);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.strikethrough {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        style
    }
}

#[derive(Debug, Clone, Copy)]
enum ListKind {
    Unordered,
    Ordered(u64),
}

#[derive(Debug, Clone, Default)]
struct TableState {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
}

impl TableState {
    fn flush_cell(&mut self) {
        self.current_row
            .push(std::mem::take(&mut self.current_cell));
    }

    fn flush_row(&mut self) {
        if !self.current_row.is_empty() || !self.current_cell.is_empty() {
            self.flush_cell();
            self.rows.push(std::mem::take(&mut self.current_row));
        }
    }

    fn finalize(&mut self) {
        self.flush_row();
    }
}

pub fn render_markdown_to_lines(
    markdown: &str,
    content_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut style = InlineStyle::default();
    let mut list_stack: Vec<(ListKind, usize)> = Vec::new();
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut in_code_block = false;
    let mut code_language = String::new();
    let mut pending_text = String::new();
    let mut heading_level: Option<u8> = None;
    let mut in_blockquote = 0usize;
    let mut table_state = TableState::default();
    let mut in_table = false;
    let mut link_url_stack: Vec<String> = Vec::new();
    let mut link_text_buf = String::new();
    let mut in_link = false;

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                heading_level = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) => {
                if !pending_text.is_empty() {
                    let mut hs = style;
                    hs.bold = true;
                    let wrapped = flush_text(
                        &pending_text,
                        hs,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                heading_level = None;
                lines.push(Line::from(""));
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                lines.push(Line::from(""));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                in_code_block = true;
                code_block_lines.clear();
                code_language = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                let highlighted = highlight_code_block(&code_block_lines, &code_language, theme);
                let lang_label = if code_language.is_empty() {
                    "text"
                } else {
                    &code_language
                };
                lines.push(Line::from(Span::styled(
                    format!("  {lang_label}"),
                    Style::default()
                        .fg(theme.code_lang)
                        .add_modifier(Modifier::BOLD),
                )));
                for hl_spans in highlighted {
                    let mut prefixed = Vec::with_capacity(hl_spans.len() + 1);
                    prefixed.push(Span::raw("  "));
                    prefixed.extend(hl_spans);
                    lines.push(Line::from(prefixed));
                }
                lines.push(Line::from(""));
                in_code_block = false;
                code_block_lines.clear();
                pending_text.clear();
            }
            Event::Text(t) => {
                if in_code_block {
                    code_block_lines.push(t.to_string());
                } else if in_table {
                    table_state.current_cell.push_str(&t);
                } else if in_link {
                    link_text_buf.push_str(&t);
                } else {
                    pending_text.push_str(&t);
                }
            }
            Event::Code(t) => {
                let saved = style.code;
                style.code = true;
                pending_text.push_str(&t);
                style.code = saved;
            }
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Strikethrough) => style.strikethrough = true,
            Event::End(TagEnd::Strikethrough) => style.strikethrough = false,
            Event::Start(Tag::List(first)) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                let kind = match first {
                    Some(n) => ListKind::Ordered(n),
                    None => ListKind::Unordered,
                };
                list_stack.push((
                    kind,
                    match first {
                        Some(n) => n as usize,
                        None => 0,
                    },
                ));
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                lines.push(Line::from(""));
            }
            Event::Start(Tag::Item) => {
                if let Some((kind, counter)) = list_stack.last_mut() {
                    if let ListKind::Ordered(_) = kind {
                        *counter += 1;
                    }
                }
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
            }
            Event::End(TagEnd::Item) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if !in_code_block {
                    pending_text.push(' ');
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                in_blockquote += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                in_blockquote = in_blockquote.saturating_sub(1);
                lines.push(Line::from(""));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_url_stack.push(dest_url.to_string());
                link_text_buf.clear();
                in_link = true;
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = link_url_stack.pop() {
                    let link_text = std::mem::take(&mut link_text_buf);
                    let link_label = format!("[{}]", url);
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            link_text,
                            Style::default()
                                .fg(theme.link)
                                .add_modifier(Modifier::UNDERLINED),
                        ),
                        Span::styled(format!(" ({})", url), Style::default().fg(theme.text_muted)),
                    ]));
                }
                in_link = false;
            }
            Event::Start(Tag::Table(_)) => {
                if !pending_text.is_empty() {
                    let wrapped = flush_text(
                        &pending_text,
                        style,
                        theme,
                        content_width,
                        &list_stack,
                        in_blockquote,
                        heading_level,
                    );
                    lines.extend(wrapped);
                    pending_text.clear();
                }
                in_table = true;
                table_state = TableState::default();
            }
            Event::End(TagEnd::Table) => {
                table_state.finalize();
                let table_lines = render_table(&table_state, content_width, theme);
                lines.extend(table_lines);
                lines.push(Line::from(""));
                in_table = false;
            }
            Event::Start(Tag::TableHead) => {}
            Event::End(TagEnd::TableHead) => {
                table_state.flush_row();
                table_state.headers = table_state.rows.pop().unwrap_or_default();
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                table_state.flush_row();
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                table_state.flush_cell();
            }
            Event::Rule => {
                lines.push(Line::from(Span::styled(
                    "\u{2500}".repeat(content_width.min(60)),
                    Style::default().fg(theme.border),
                )));
                lines.push(Line::from(""));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                pending_text.push_str(&html);
            }
            Event::FootnoteReference(_) => {}
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[\u{2713}] " } else { "[ ] " };
                pending_text.push_str(marker);
            }
            Event::InlineMath(_) | Event::DisplayMath(_) | _ => {}
        }
    }

    if !pending_text.is_empty() {
        let wrapped = flush_text(
            &pending_text,
            style,
            theme,
            content_width,
            &list_stack,
            in_blockquote,
            heading_level,
        );
        lines.extend(wrapped);
    }

    if lines.last().map_or(false, |l| l.width() == 0) && lines.len() > 1 {
        lines.pop();
    }

    lines
}

fn flush_text(
    text: &str,
    style: InlineStyle,
    theme: &Theme,
    width: usize,
    list_stack: &[(ListKind, usize)],
    blockquote_depth: usize,
    heading_lvl: Option<u8>,
) -> Vec<Line<'static>> {
    if text.is_empty() {
        return Vec::new();
    }

    let is_blockquote = blockquote_depth > 0;
    let is_list = !list_stack.is_empty();

    let mut prefix_str = String::new();
    let prefix_style = if is_list {
        for _ in 0..blockquote_depth {
            prefix_str.push('\u{2502}');
            prefix_str.push(' ');
        }
        let indent_levels = list_stack.len().saturating_sub(1);
        prefix_str.push_str(&"  ".repeat(indent_levels));
        if let Some((kind, counter)) = list_stack.last() {
            match kind {
                ListKind::Unordered => {
                    prefix_str.push('\u{2022}');
                    prefix_str.push(' ');
                }
                ListKind::Ordered(_) => {
                    prefix_str.push_str(&format!("{}. ", *counter));
                }
            }
        }
        if let Some((kind, _)) = list_stack.last() {
            match kind {
                ListKind::Unordered => Style::default().fg(theme.list_item),
                ListKind::Ordered(_) => Style::default().fg(theme.list_enum),
            }
        } else {
            style.to_ratatui(theme)
        }
    } else if is_blockquote {
        prefix_str = "\u{2502} ".repeat(blockquote_depth);
        Style::default().fg(theme.blockquote)
    } else {
        style.to_ratatui(theme)
    };

    let prefix_width: usize = prefix_str.chars().map(|c| c.width().unwrap_or(1)).sum();

    let effective_width = width.saturating_sub(prefix_width).max(10);

    let text_style = if let Some(hl) = heading_lvl {
        style.to_ratatui(theme).fg(match hl {
            1 => theme.heading_1,
            2 => theme.heading_2,
            _ => theme.heading_3,
        })
    } else {
        style.to_ratatui(theme)
    };

    let full_line = Line::from(vec![
        Span::styled(prefix_str, prefix_style),
        Span::styled(text.to_string(), text_style),
    ]);

    if full_line.width() <= effective_width {
        return vec![full_line];
    }

    wrap_line(full_line, effective_width)
}

fn wrap_line(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    if line.width() <= max_width {
        return vec![line];
    }

    let mut result = Vec::new();
    let prefix_span = if line.spans.first().map_or(false, |s| !s.content.is_empty()) {
        Some(line.spans[0].clone())
    } else {
        None
    };

    let spans_to_wrap = if prefix_span.is_some() {
        &line.spans[1..]
    } else {
        &line.spans[..]
    };

    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0;

    for span in spans_to_wrap {
        let span_text: String = span.content.clone().into_owned();
        let span_w = span_text
            .chars()
            .map(|c| c.width().unwrap_or(1))
            .sum::<usize>();

        if current_width + span_w <= max_width {
            current_spans.push(Span::styled(span_text, span.style));
            current_width += span_w;
        } else {
            let remaining = span_text;
            let mut accum = String::new();
            let mut accum_width = 0usize;

            for ch in remaining.chars() {
                let cw = ch.width().unwrap_or(1);
                if current_width + accum_width + cw > max_width && !current_spans.is_empty() {
                    if !accum.is_empty() {
                        current_spans.push(Span::styled(accum.clone(), span.style));
                        accum.clear();
                        accum_width = 0;
                    }
                    let mut next_spans = if let Some(ref p) = prefix_span {
                        vec![p.clone()]
                    } else {
                        Vec::new()
                    };
                    next_spans.extend(current_spans.drain(..));
                    result.push(Line::from(next_spans));
                    current_width = 0;
                }
                accum.push(ch);
                accum_width += cw;
            }
            if !accum.is_empty() {
                current_spans.push(Span::styled(accum, span.style));
                current_width += accum_width;
            }
        }
    }

    if !current_spans.is_empty() {
        let mut final_line = if let Some(ref p) = prefix_span {
            vec![p.clone()]
        } else {
            Vec::new()
        };
        final_line.extend(current_spans);
        result.push(Line::from(final_line));
    }

    result
}

fn syntect_style_to_ratatui(s: &syntect::highlighting::Style) -> Style {
    let fg = Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b);
    let bg = Color::Rgb(s.background.r, s.background.g, s.background.b);
    let mut style = Style::default().fg(fg).bg(bg);
    if s.font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

fn highlight_code_block(
    lines: &[String],
    language: &str,
    theme: &Theme,
) -> Vec<Vec<Span<'static>>> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let syntax = syntax_set
        .find_syntax_by_token(language)
        .or_else(|| syntax_set.find_syntax_by_token("text"))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let theme_set = ThemeSet::load_defaults();
    let syntect_theme = theme_set
        .themes
        .get("base16-ocean.dark")
        .unwrap_or_else(|| &theme_set.themes["InspiredGitHub"]);

    let mut highlighter = HighlightLines::new(syntax, syntect_theme);
    let mut result = Vec::new();

    for line_text in lines {
        let line_with_newline = format!("{line_text}\n");
        let highlighted = highlighter.highlight_line(&line_with_newline, &syntax_set);
        if let Ok(ranges) = highlighted {
            let mut spans = Vec::new();
            for (s, text) in ranges {
                let trimmed = text.trim_end_matches('\n');
                if !trimmed.is_empty() {
                    spans.push(Span::styled(
                        trimmed.to_string(),
                        syntect_style_to_ratatui(&s),
                    ));
                }
            }
            if spans.is_empty() {
                spans.push(Span::styled(
                    line_text.clone(),
                    Style::default().fg(theme.code_text).bg(theme.code_bg),
                ));
            }
            result.push(spans);
        } else {
            result.push(vec![Span::styled(
                line_text.clone(),
                Style::default().fg(theme.code_text).bg(theme.code_bg),
            )]);
        }
    }

    result
}

fn render_table(state: &TableState, content_width: usize, theme: &Theme) -> Vec<Line<'static>> {
    if state.headers.is_empty() && state.rows.is_empty() {
        return vec![];
    }

    let col_count = state
        .headers
        .len()
        .max(state.rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if col_count == 0 {
        return vec![];
    }

    let mut col_widths = vec![3usize; col_count];
    for (i, h) in state.headers.iter().enumerate() {
        if i < col_count {
            col_widths[i] =
                col_widths[i].max(h.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>());
        }
    }
    for row in &state.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                col_widths[i] =
                    col_widths[i].max(cell.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>());
            }
        }
    }

    let total_width: usize = col_widths.iter().sum::<usize>() + col_count + 1;
    if total_width > content_width {
        let scale = content_width as f64 / total_width as f64;
        for w in &mut col_widths {
            *w = (*w as f64 * scale).ceil().max(3.0) as usize;
        }
    }

    let mut result = Vec::new();
    let vbar = '\u{2502}';

    let mut header_line = String::new();
    header_line.push(vbar);
    for (i, &w) in col_widths.iter().enumerate() {
        let header_text = state.headers.get(i).map(|s| s.as_str()).unwrap_or("");
        let padded = pad_right(header_text, w);
        header_line.push(' ');
        header_line.push_str(&padded);
        header_line.push(' ');
        header_line.push(vbar);
    }
    result.push(Line::from(Span::styled(
        header_line,
        Style::default()
            .fg(theme.heading_2)
            .add_modifier(Modifier::BOLD),
    )));

    let mut sep_line = String::new();
    sep_line.push(vbar);
    for &w in &col_widths {
        sep_line.push_str("\u{2500}".repeat(w + 2).as_str());
        sep_line.push(vbar);
    }
    result.push(Line::from(Span::styled(
        sep_line,
        Style::default().fg(theme.border),
    )));

    for row in &state.rows {
        let mut row_line = String::new();
        row_line.push(vbar);
        for (i, &w) in col_widths.iter().enumerate() {
            let cell_text = row.get(i).map(|s| s.as_str()).unwrap_or("");
            let padded = pad_right(cell_text, w);
            row_line.push(' ');
            row_line.push_str(&padded);
            row_line.push(' ');
            row_line.push(vbar);
        }
        result.push(Line::from(Span::styled(
            row_line,
            Style::default().fg(theme.text),
        )));
    }

    result
}

fn pad_right(s: &str, width: usize) -> String {
    let current_width: usize = s.chars().map(|c| c.width().unwrap_or(1)).sum();
    let padding = width.saturating_sub(current_width);
    format!("{}{}", s, " ".repeat(padding))
}
