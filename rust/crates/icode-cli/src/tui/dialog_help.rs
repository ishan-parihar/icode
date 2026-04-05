use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone)]
struct HelpKeyBinding {
    key: &'static str,
    desc: &'static str,
}

#[derive(Debug, Clone)]
struct HelpSection<'a> {
    title: &'a str,
    bindings: &'a [HelpKeyBinding],
}

#[derive(Debug, Clone)]
pub struct HelpDialogState {
    pub open: bool,
    pub cursor: usize,
}

impl HelpDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: 0,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.cursor = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.cursor = 0;
    }

    pub fn cursor_up(&mut self, _total: usize) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self, total: usize) {
        if self.cursor + 1 < total {
            self.cursor += 1;
        }
    }
}

impl Default for HelpDialogState {
    fn default() -> Self {
        Self::new()
    }
}

fn dialog_width(term_width: u16) -> u16 {
    ((term_width as f32 * 0.5) as u16).clamp(50, 70)
}

fn dialog_height(term_height: u16) -> u16 {
    (term_height as f32 * 0.65).clamp(18.0, 32.0) as u16
}

fn sections() -> Vec<HelpSection<'static>> {
    vec![
        HelpSection {
            title: "Navigation",
            bindings: &[
                HelpKeyBinding {
                    key: "\u{2191}/\u{2193}",
                    desc: "Scroll messages",
                },
                HelpKeyBinding {
                    key: "Tab",
                    desc: "Complete prompt",
                },
                HelpKeyBinding {
                    key: "Esc",
                    desc: "Close dialogs",
                },
            ],
        },
        HelpSection {
            title: "Session",
            bindings: &[
                HelpKeyBinding {
                    key: "Ctrl+M",
                    desc: "Switch model",
                },
                HelpKeyBinding {
                    key: "Ctrl+X L",
                    desc: "Switch session",
                },
                HelpKeyBinding {
                    key: "Ctrl+L",
                    desc: "Clear conversation",
                },
                HelpKeyBinding {
                    key: "Alt+S",
                    desc: "Toggle sidebar",
                },
                HelpKeyBinding {
                    key: "PgUp/PgDn",
                    desc: "Undo/Redo",
                },
            ],
        },
        HelpSection {
            title: "Commands",
            bindings: &[
                HelpKeyBinding {
                    key: "Ctrl+Space",
                    desc: "Open command palette",
                },
                HelpKeyBinding {
                    key: "/",
                    desc: "Search (in dialogs)",
                },
                HelpKeyBinding {
                    key: "Enter",
                    desc: "Select/confirm",
                },
            ],
        },
        HelpSection {
            title: "Theme",
            bindings: &[
                HelpKeyBinding {
                    key: "(cmd palette)",
                    desc: "Toggle theme",
                },
                HelpKeyBinding {
                    key: "(cmd palette)",
                    desc: "Switch theme",
                },
            ],
        },
        HelpSection {
            title: "System",
            bindings: &[HelpKeyBinding {
                key: "Ctrl+C",
                desc: "Exit",
            }],
        },
    ]
}

fn total_bindings(sections: &[HelpSection<'_>]) -> usize {
    sections.iter().map(|s| s.bindings.len()).sum()
}

pub fn render_help_dialog(frame: &mut Frame, state: &HelpDialogState, area: Rect, theme: Theme) {
    if !state.open {
        return;
    }

    let sections = sections();

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .title(Span::styled(
            " Keybindings ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);

    frame.render_widget(block.clone(), dialog_area);
    let inner = dialog_area.inner(Margin::new(1, 1));

    // Compute scroll offset so cursor stays visible
    let visible = inner.height.saturating_sub(2) as usize; // reserve footer
    let scroll = if state.cursor >= visible {
        state.cursor - visible + 1
    } else {
        0
    };

    let mut lines: Vec<Line<'_>> = Vec::new();
    let mut global_idx: usize = 0;

    for section in &sections {
        lines.push(Line::from(Span::styled(
            format!(" {} ", section.title),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )));
        for binding in section.bindings {
            let is_cursor = global_idx == state.cursor;
            let key_style = if is_cursor {
                Style::default()
                    .bg(theme.background_hover)
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_muted)
            };
            let desc_style = if is_cursor {
                Style::default()
                    .bg(theme.background_hover)
                    .fg(theme.text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(format!("{:<14}", binding.key), key_style),
                Span::raw(" \u{2502} "),
                Span::styled(binding.desc, desc_style),
            ]));
            global_idx += 1;
        }
    }

    // Apply scroll offset and take only visible lines
    let displayed: Vec<Line<'_>> = lines.into_iter().skip(scroll).take(visible).collect();

    // Pad remaining visible space so footer stays at bottom
    let padding = visible.saturating_sub(displayed.len());
    let mut padded = displayed;
    for _ in 0..padding {
        padded.push(Line::raw(""));
    }

    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(Paragraph::new(padded), content_chunks[0]);

    let hint = Span::styled(
        " \u{2191}/\u{2193} navigate  \u{2022}  Esc: close ",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), content_chunks[1]);
}
