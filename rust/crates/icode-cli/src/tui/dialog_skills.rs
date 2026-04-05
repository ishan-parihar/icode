use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 16;

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        116
    } else if term_width >= 96 {
        88
    } else {
        MIN_WIDTH
    }
}
fn dialog_height(term_height: u16) -> u16 {
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub description: Option<String>,
    pub source: String,
    pub shadowed_by: Option<String>,
}

pub struct SkillsDialogState {
    pub open: bool,
    pub skills: Vec<SkillEntry>,
    pub cursor: usize,
    pub search: String,
    pub filtered: Vec<usize>,
    pub section_offsets: Vec<(String, usize)>,
}

impl SkillsDialogState {
    pub fn new() -> Self {
        let skills = discover_skills();
        Self {
            open: false,
            skills,
            cursor: 0,
            search: String::new(),
            filtered: Vec::new(),
            section_offsets: Vec::new(),
        }
    }

    pub fn open(&mut self) {
        self.skills = discover_skills();
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    pub fn type_char(&mut self, c: char) {
        self.search.push(c);
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn backspace(&mut self) {
        self.search.pop();
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn rebuild_filtered(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered.clear();
        self.section_offsets.clear();

        let matches_query = |s: &SkillEntry| -> bool {
            if query.is_empty() {
                return true;
            }
            s.name.to_lowercase().contains(&query)
                || s.description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&query))
                || s.source.to_lowercase().contains(&query)
        };

        let is_project = |s: &SkillEntry| -> bool { s.source.starts_with("project") };
        let is_user = |s: &SkillEntry| -> bool { s.source.starts_with("user") };
        let is_global = |s: &SkillEntry| -> bool { s.source.starts_with("global") };

        let project_skills: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| is_project(s) && matches_query(s))
            .map(|(i, _)| i)
            .collect();

        let user_skills: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| is_user(s) && matches_query(s))
            .map(|(i, _)| i)
            .collect();

        let global_skills: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| is_global(s) && matches_query(s))
            .map(|(i, _)| i)
            .collect();

        if !project_skills.is_empty() {
            self.section_offsets
                .push(("Project Skills".to_string(), self.filtered.len()));
            self.filtered.extend(project_skills);
        }

        if !user_skills.is_empty() {
            self.section_offsets
                .push(("User Skills".to_string(), self.filtered.len()));
            self.filtered.extend(user_skills);
        }

        if !global_skills.is_empty() {
            self.section_offsets
                .push(("Global Skills".to_string(), self.filtered.len()));
            self.filtered.extend(global_skills);
        }

        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn current_section(&self) -> &str {
        self.section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| self.cursor >= *offset)
            .map(|(name, _)| name.as_str())
            .unwrap_or("Skills")
    }
}

impl Default for SkillsDialogState {
    fn default() -> Self {
        Self::new()
    }
}

fn discover_skills() -> Vec<SkillEntry> {
    let mut all: Vec<(String, SkillEntry)> = Vec::new();
    let mut dirs: Vec<(String, std::path::PathBuf)> = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        let codex_skills = cwd.join(".codex").join("skills");
        if codex_skills.is_dir() {
            dirs.push(("project".to_string(), codex_skills));
        }
        let claude_skills = cwd.join(".claude").join("skills");
        if claude_skills.is_dir() {
            dirs.push(("project".to_string(), claude_skills));
        }
    }
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let p = std::path::PathBuf::from(&codex_home).join("skills");
        if p.is_dir() {
            dirs.push(("project".to_string(), p));
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::PathBuf::from(home);
        let agents_skills = home.join(".agents").join("skills");
        if agents_skills.is_dir() {
            dirs.push(("user".to_string(), agents_skills));
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::PathBuf::from(home);
        let opencode_skills = home.join(".config").join("opencode").join("skills");
        if opencode_skills.is_dir() {
            dirs.push(("global".to_string(), opencode_skills));
        }
        let codex_skills = home.join(".codex").join("skills");
        if codex_skills.is_dir() {
            dirs.push(("global".to_string(), codex_skills));
        }
    }

    for (source_label, dir) in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let description = std::fs::read_to_string(&skill_md)
                    .ok()
                    .as_deref()
                    .and_then(parse_skill_description);
                let source_path = path.display().to_string();
                let source = format!("{source_label} | {source_path}");

                all.push((
                    name.clone(),
                    SkillEntry {
                        name,
                        description,
                        source,
                        shadowed_by: None,
                    },
                ));
            }
        }
    }

    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (_, skill) in &mut all {
        if let Some(winner) = seen.get(&skill.name) {
            skill.shadowed_by = Some(winner.clone());
        } else {
            seen.insert(skill.name.clone(), skill.name.clone());
        }
    }

    all.into_iter().map(|(_, s)| s).collect()
}

fn parse_skill_description(contents: &str) -> Option<String> {
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("description:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn render_skills_dialog(
    frame: &mut Frame,
    state: &SkillsDialogState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Skills ")
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(block, dialog_area);

    let inner = dialog_area.inner(Margin::new(1, 1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let search_prompt = if state.search.is_empty() {
        Span::styled("Type to filter...", Style::default().fg(theme.text_muted))
    } else {
        Span::raw(&state.search)
    };
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled("/ ", Style::default().fg(theme.accent)),
        search_prompt,
    ]));
    frame.render_widget(search_para, chunks[0]);

    let visible_lines = chunks[1].height as usize;
    let scroll_offset = compute_scroll_offset(state, visible_lines);

    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = String::new();

    for (pos, &skill_idx) in state.filtered.iter().enumerate() {
        if pos < scroll_offset {
            continue;
        }
        if lines.len() >= visible_lines {
            break;
        }

        let skill = &state.skills[skill_idx];

        let section = state
            .section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| pos >= *offset)
            .map(|(name, _)| name.as_str())
            .unwrap_or("Skills");

        if section != current_section {
            current_section = section.to_string();
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  -- {section} --"),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let is_selected = pos == state.cursor;
        let is_shadowed = skill.shadowed_by.is_some();

        let base_style = if is_selected {
            Style::default()
                .bg(theme.background_hover)
                .fg(theme.text_inverse)
                .add_modifier(Modifier::BOLD)
        } else if is_shadowed {
            Style::default().fg(theme.text_muted)
        } else {
            Style::default().fg(theme.text)
        };

        let marker = if is_selected { "\u{25b6} " } else { "  " };

        let mut row_spans: Vec<Span> = Vec::new();

        row_spans.push(Span::styled(marker, base_style));

        if is_shadowed {
            if let Some(shadow_name) = &skill.shadowed_by {
                row_spans.push(Span::styled(
                    format!("(shadowed by {shadow_name}) "),
                    Style::default().fg(theme.error),
                ));
            }
        }

        row_spans.push(Span::styled(&skill.name, base_style));

        if let Some(desc) = &skill.description {
            let desc_style = if is_selected {
                Style::default()
                    .bg(theme.background_hover)
                    .fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text_muted)
            };
            row_spans.push(Span::styled(format!(" \u{b7} {desc}"), desc_style));
        }

        let source_short = shorten_source(&skill.source, 30);
        let source_style = if is_selected {
            Style::default()
                .bg(theme.background_hover)
                .fg(theme.text_muted)
        } else {
            Style::default().fg(theme.syntax_comment)
        };
        row_spans.push(Span::styled(format!(" [{source_short}]"), source_style));

        lines.push(Line::from(row_spans));
    }

    if state.filtered.is_empty() {
        let empty_msg = if state.search.is_empty() {
            "No skills found"
        } else {
            "No skills match search"
        };
        lines.push(Line::from(Span::styled(
            empty_msg,
            Style::default().fg(theme.text_muted),
        )));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    let help_text = " \u{2191}\u{2193} navigate  Esc: close  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &SkillsDialogState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}

fn shorten_source(source: &str, max_len: usize) -> String {
    if source.len() <= max_len {
        return source.to_string();
    }
    format!("...{}", &source[source.len() - max_len + 3..])
}
