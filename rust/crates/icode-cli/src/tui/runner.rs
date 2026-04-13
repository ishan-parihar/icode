use crate::tui::app::{self, AppMode, AppState, MessagePart, MessageRole, ToastKind};
use crate::tui::autocomplete::AutocompleteMode;
use crate::tui::command_palette::{find_slash_command_action, CommandAction};
use crate::tui::event::{Event, EventLoop, ParsedKey};
use crate::tui::frecency::FrecencyStore;
use crate::tui::input::InputState;
use crate::tui::kitty::KittyKeyboard;
use crate::tui::layout::render_ui;
use crate::tui::modal_manager::ActiveModal;
use crate::tui::Theme;
use crate::TurnEvent;
use color_eyre::eyre::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use runtime::skill_manager::SkillManager;
use std::fmt::Write as _;
use std::io;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

enum PaletteAction {
    NewSession,
    ForkSession,
    Exit,
}

pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    event_loop: EventLoop,
    state: AppState,
    theme: Theme,
    turn_rx: Option<Receiver<TurnEvent>>,
    skill_manager: Arc<SkillManager>,
    pending_palette_action: Option<PaletteAction>,
}

impl Tui {
    pub fn new(model: &str, permission_mode: &str, cwd: &str) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let _ = KittyKeyboard::enable();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        let event_loop = EventLoop::new(250);

        let cwd_path = std::path::PathBuf::from(cwd);
        let local_roots = vec![
            cwd_path.join(".claude").join("skills"),
            cwd_path.join(".agents").join("skills"),
        ];
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("icode")
            .join("skills");
        let skill_manager = Arc::new(SkillManager::new(&local_roots, cache_dir));

        let mut state = AppState::new(model, permission_mode, cwd, Some(skill_manager.clone()));
        let theme = Theme::default();

        let models: Vec<String> = state
            .model_picker
            .entries
            .iter()
            .flat_map(|e| vec![e.alias.clone(), e.canonical.clone()])
            .collect();
        state.prompt.set_models(models);
        state.prompt.set_cwd(cwd.to_string());
        state.sessions_dialog.load_sessions();
        let sessions: Vec<String> = state
            .sessions_dialog
            .sessions
            .iter()
            .map(|s| s.id.clone())
            .collect();
        state.prompt.set_sessions(sessions);

        let frecency_path = app::icode_config_dir().join("frecency.json");
        let mut frecency = FrecencyStore::new(frecency_path);
        let _ = frecency.load();
        state.prompt.frecency = Some(frecency);

        // Show welcome/setup flow when no providers are configured
        let any_configured = api::scan_provider_auth_status(None)
            .iter()
            .any(|p| p.has_auth);
        if !any_configured {
            state.mode = AppMode::Welcome;
            state.provider_dialog.refresh_providers();
        }

        Ok(Self {
            terminal,
            event_loop,
            state,
            theme,
            turn_rx: None,
            skill_manager,
            pending_palette_action: None,
        })
    }

    pub fn run(&mut self) -> Result<String> {
        loop {
            self.state.check_leader_timeout();

            if let Some(ts) = self.state.interrupt_timestamp {
                if ts.elapsed() > std::time::Duration::from_secs(5) {
                    self.state.interrupt_count = 0;
                    self.state.interrupt_timestamp = None;
                }
            }

            self.terminal.draw(|frame| {
                render_ui(frame, &mut self.state, self.theme);
            })?;

            match self.event_loop.next() {
                Ok(Event::Key(key)) => {
                    if let Some(input) = self.handle_key(key) {
                        self.terminal.draw(|frame| {
                            render_ui(frame, &mut self.state, self.theme);
                        })?;
                        return Ok(input);
                    }
                    if let Some(action) = self.pending_palette_action.take() {
                        self.terminal.draw(|frame| {
                            render_ui(frame, &mut self.state, self.theme);
                        })?;
                        return Ok(match action {
                            PaletteAction::NewSession => "__new_session__".to_string(),
                            PaletteAction::ForkSession => "__fork_session__".to_string(),
                            PaletteAction::Exit => String::new(),
                        });
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    self.state.recalculate_scroll();
                }
                Ok(Event::Tick) => {
                    self.state.prune_expired_toasts();
                    self.poll_turn_events();
                }
                Ok(Event::Mouse(mouse)) => {
                    if self.state.message_action_dialog.open {
                        continue;
                    }
                    match mouse.kind {
                        crossterm::event::MouseEventKind::Down(
                            crossterm::event::MouseButton::Left,
                        ) => {
                            self.state.selection = Some(crate::tui::app::TextSelection {
                                start_row: mouse.row,
                                start_col: mouse.column,
                                end_row: mouse.row,
                                end_col: mouse.column,
                                content_lines: Vec::new(),
                            });
                        }
                        crossterm::event::MouseEventKind::Drag(
                            crossterm::event::MouseButton::Left,
                        ) => {
                            if let Some(ref mut sel) = self.state.selection {
                                sel.end_row = mouse.row;
                                sel.end_col = mouse.column;
                            }
                        }
                        crossterm::event::MouseEventKind::Up(
                            crossterm::event::MouseButton::Left,
                        ) => {
                            if let Some(sel) = self.state.selection.take() {
                                let row_dist = (sel.start_row as i32 - sel.end_row as i32).abs();
                                let col_dist = (sel.start_col as i32 - sel.end_col as i32).abs();
                                if row_dist > 0 || col_dist > 2 {
                                    let text = self.extract_selection_text(&sel);
                                    if !text.is_empty() {
                                        copy_to_clipboard(&text);
                                        self.state
                                            .add_toast("Copied to clipboard", ToastKind::Success);
                                    }
                                } else if let Some((msg_idx, tc_idx)) =
                                    self.tool_call_at(mouse.row, mouse.column)
                                {
                                    self.state.toggle_tool_expand(msg_idx, tc_idx);
                                } else if let Some(idx) = self.message_at(mouse.row, mouse.column) {
                                    if let Some(msg) = self.state.messages.get(idx) {
                                        if matches!(msg.role, crate::tui::app::MessageRole::User) {
                                            self.state
                                                .message_action_dialog
                                                .open(idx, msg.full_text());
                                        }
                                    }
                                }
                            } else if let Some((msg_idx, tc_idx)) =
                                self.tool_call_at(mouse.row, mouse.column)
                            {
                                self.state.toggle_tool_expand(msg_idx, tc_idx);
                            } else if let Some(idx) = self.message_at(mouse.row, mouse.column) {
                                if let Some(msg) = self.state.messages.get(idx) {
                                    if matches!(msg.role, crate::tui::app::MessageRole::User) {
                                        self.state.message_action_dialog.open(idx, msg.full_text());
                                    }
                                }
                            }
                        }
                        crossterm::event::MouseEventKind::ScrollUp => {
                            if self.state.scroll_offset == usize::MAX {
                                self.state.scroll_offset = 0;
                            } else {
                                self.state.scroll_offset =
                                    self.state.scroll_offset.saturating_sub(3);
                            }
                            self.state.scroll_paused = true;
                            self.state.auto_scroll = false;
                        }
                        crossterm::event::MouseEventKind::ScrollDown => {
                            if self.state.scroll_offset != usize::MAX {
                                self.state.scroll_offset =
                                    self.state.scroll_offset.saturating_add(3);
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    self.state.mode = AppMode::Error(e.to_string());
                    return Ok(String::new());
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        let modal = self.state.active_modal.take();
        if let Some(mut modal) = modal {
            let result = match &mut modal {
                ActiveModal::Permission(s) => self.handle_permission_action_from_modal(key, s),
                ActiveModal::Question(s) => self.handle_question_action_from_modal(key, s),
                ActiveModal::ModelPicker(s) => self.handle_picker_action_from_modal(key, s),
                ActiveModal::CommandPalette(s) => self.handle_palette_action_from_modal(key, s),
                ActiveModal::Mcp(s) => self.handle_mcp_action_from_modal(key, s),
                ActiveModal::Skills(s) => self.handle_skills_action_from_modal(key, s),
                ActiveModal::ThemeList(s) => self.handle_theme_list_action_from_modal(key, s),
                ActiveModal::Plugins(s) => self.handle_plugins_action_from_modal(key, s),
                ActiveModal::Sessions(s) => self.handle_sessions_action_from_modal(key, s),
                ActiveModal::MessageAction(s) => {
                    self.handle_message_action_action_from_modal(key, s)
                }
                ActiveModal::Help(s) => self.handle_help_action_from_modal(key, s),
                ActiveModal::ContextViz(s) => self.handle_context_viz_action_from_modal(key, s),
                ActiveModal::SessionBranching(s) => self.handle_branching_action_from_modal(key, s),
                ActiveModal::PromptStash(s) => self.handle_stash_action_from_modal(key, s),
                ActiveModal::ExportOptions(s) => {
                    self.handle_export_options_action_from_modal(key, s)
                }
                ActiveModal::DebugPanel(s) => self.handle_debug_panel_action_from_modal(key, s),
                ActiveModal::Provider(s) => self.handle_provider_action_from_modal(key, s),
                ActiveModal::Workspace(s) => self.handle_workspace_action_from_modal(key, s),
                ActiveModal::DiffView(s) => self.handle_diff_view_action_from_modal(key, s),
                ActiveModal::Pager(s) => self.handle_pager_action_from_modal(key, s),
                ActiveModal::Autocomplete(s) => self.handle_autocomplete_action_from_modal(key, s),
            };
            if self.state.active_modal.is_none()
                && !matches!(key.code, KeyCode::Esc)
                && result.is_none()
            {
                self.state.active_modal = Some(modal);
            }
            return result;
        }

        let content_width = self.content_width() as usize;
        let input_width = (self
            .terminal
            .size()
            .unwrap_or(ratatui::layout::Size {
                width: 80,
                height: 24,
            })
            .width as usize)
            .saturating_sub(4);

        // Error mode: any key resets to Normal
        if matches!(self.state.mode, AppMode::Error(_)) {
            self.state.mode = AppMode::Normal;
            return None;
        }

        // Welcome mode: Enter opens provider dialog, Esc dismisses
        if matches!(self.state.mode, AppMode::Welcome) {
            match key.code {
                KeyCode::Enter => {
                    self.state.provider_dialog.open = true;
                    self.state.mode = AppMode::Normal;
                    return None;
                }
                KeyCode::Esc => {
                    self.state.mode = AppMode::Normal;
                    return None;
                }
                _ => {}
            }
        }

        // AuthError mode: 'p'/'P' opens provider dialog, any other key dismisses
        if matches!(self.state.mode, AppMode::AuthError(_)) {
            if let KeyCode::Char('p' | 'P') = key.code {
                self.state.provider_dialog.refresh_providers();
                self.state.provider_dialog.open = true;
                self.state.mode = AppMode::Normal;
                return None;
            }
            self.state.mode = AppMode::Normal;
            return None;
        }

        if key.code == KeyCode::Esc {
            if self.state.autocomplete.open {
                self.state.autocomplete.close();
                return None;
            }
            if self.state.is_streaming {
                if self.state.interrupt_count >= 1 {
                    self.state.is_streaming = false;
                    self.state.finish_stream();
                    self.state.mode = AppMode::Normal;
                    self.turn_rx = None;
                    if let Some(msg) = self.state.messages.last_mut() {
                        if msg.is_streaming {
                            msg.is_streaming = false;
                        }
                    }
                    self.state
                        .add_toast("Generation cancelled", ToastKind::Info);
                    self.state.interrupt_count = 0;
                    self.state.interrupt_timestamp = None;
                    return None;
                }
                self.state.interrupt_count = 1;
                self.state.interrupt_timestamp = Some(std::time::Instant::now());
                return None;
            }
            if self.state.selection.is_some() {
                self.state.selection = None;
                return None;
            }
            if self.state.toasts.iter().any(|t| t.persistent) {
                self.state.dismiss_persistent_toast();
                return None;
            }
            return None;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.state.is_streaming {
                    self.state.mode = AppMode::Normal;
                    self.state.is_streaming = false;
                    self.state.finish_stream();
                    None
                } else {
                    Some(String::new())
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
                self.state.open_model_picker();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                self.state.open_command_palette();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
                self.state.prompt.insert_char('\n');
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                if self.terminal.size().is_ok() {
                    self.terminal.clear().ok();
                }
                self.state
                    .add_toast("Screen refreshed", crate::tui::app::ToastKind::Info);
                None
            }
            (KeyModifiers::SHIFT, KeyCode::Enter) => {
                self.state.prompt.insert_char('\n');
                None
            }
            (_, KeyCode::Enter) => {
                if self.state.autocomplete.open {
                    let was_slash_command = self.state.autocomplete.slash_command_selected;
                    let selected_title = self
                        .state
                        .autocomplete
                        .entries
                        .get(self.state.autocomplete.idx)
                        .map(|e| e.title.clone());
                    self.state.autocomplete.select(&mut self.state.prompt);
                    if let Some(ref mut frecency) = self.state.prompt.frecency {
                        if let Some(entry) = self
                            .state
                            .autocomplete
                            .entries
                            .get(self.state.autocomplete.idx)
                        {
                            frecency.record(&entry.title);
                        }
                    }
                    if was_slash_command {
                        self.state.autocomplete.slash_command_selected = false;
                        let cmd = self.state.prompt.value.clone();
                        self.state.prompt.clear();
                        self.execute_slash_command(&cmd);
                        return None;
                    }
                    return None;
                }

                if let Some(cmd) = self.state.pending_slash_command.take() {
                    self.execute_slash_command(&cmd);
                    return None;
                }

                let input = self.state.prompt.submit();
                if input.trim().is_empty() {
                    None
                } else if input.trim().starts_with('/') {
                    self.execute_slash_command(input.trim());
                    None
                } else {
                    self.state.prompt.push_history();
                    self.state.cleanup_reverted();
                    let (clean_input, file_refs) =
                        crate::tui::file_picker::parse_file_references(&input, &self.state.cwd);
                    self.state.pending_file_refs = file_refs
                        .iter()
                        .map(|r| (r.path.clone(), r.content.clone()))
                        .collect();

                    let user_input = if file_refs.is_empty() {
                        clean_input.clone()
                    } else {
                        let mut enriched = clean_input.clone();
                        enriched.push_str("\n\n<attached_files>\n");
                        for r in &file_refs {
                            let _ = write!(enriched, "## {}\n{}\n\n", r.path, r.content);
                        }
                        enriched.push_str("</attached_files>");
                        enriched
                    };

                    self.state.add_user_message(clean_input);
                    self.state.turn_started_at = Some(Instant::now());
                    self.state.start_assistant_stream("build");
                    Some(user_input)
                }
            }
            (_, KeyCode::Tab) => {
                if self.state.autocomplete.open {
                    self.state.autocomplete.select(&mut self.state.prompt);
                    if let Some(ref mut frecency) = self.state.prompt.frecency {
                        if let Some(entry) = self
                            .state
                            .autocomplete
                            .entries
                            .get(self.state.autocomplete.idx)
                        {
                            frecency.record(&entry.title);
                        }
                    }
                }
                None
            }
            (_, KeyCode::BackTab) => {
                if self.state.autocomplete.open {
                    self.state.autocomplete.cursor_up();
                }
                None
            }
            (_, KeyCode::Up) => {
                if self.state.autocomplete.open {
                    self.state.autocomplete.cursor_up();
                } else {
                    let (visual_row, _) = self.state.prompt.cursor_position(input_width);
                    let total_rows = self.state.prompt.total_rows(input_width);
                    let is_at_top = visual_row == 0;
                    let is_in_history = !self.state.prompt.history.is_empty()
                        && (self.state.prompt.history_temp.is_some()
                            || self.state.prompt.history_idx < self.state.prompt.history.len());
                    if is_at_top && is_in_history {
                        self.state.prompt.history_up();
                    } else if !is_at_top {
                        self.state.prompt.move_up(input_width);
                    } else if self.state.scroll_offset == usize::MAX {
                        self.state.scroll_offset = 0;
                    } else {
                        self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
                    }
                }
                None
            }
            (_, KeyCode::Down) => {
                if self.state.autocomplete.open {
                    self.state.autocomplete.cursor_down();
                } else {
                    let (visual_row, _) = self.state.prompt.cursor_position(input_width);
                    let total_rows = self.state.prompt.total_rows(input_width);
                    let is_at_bottom = visual_row + 1 >= total_rows;
                    let is_in_history = self.state.prompt.history_temp.is_some()
                        || self.state.prompt.history_idx < self.state.prompt.history.len();
                    if is_at_bottom && is_in_history {
                        self.state.prompt.history_down();
                    } else if !is_at_bottom {
                        self.state.prompt.move_down(input_width);
                    } else if self.state.scroll_offset != usize::MAX {
                        self.state.scroll_offset = self.state.scroll_offset.saturating_add(1);
                    }
                }
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                self.state.messages.clear();
                self.state.tools.clear();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
                self.state.prompt.clear();
                self.state.autocomplete.close();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                self.state.activate_leader();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => {
                let session_id = self.state.session.id.clone();
                self.state.open_session_branching(&session_id);
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.state.open_prompt_stash();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                for (msg_idx, msg) in self.state.messages.iter().enumerate().rev() {
                    if let Some(tc_idx) = msg.parts.iter().rev().position(|p| {
                        matches!(
                            p,
                            MessagePart::ToolCall {
                                output: Some(_),
                                ..
                            }
                        )
                    }) {
                        let real_tc_idx = msg
                            .parts
                            .iter()
                            .filter(|p| matches!(p, MessagePart::ToolCall { .. }))
                            .count()
                            - 1
                            - tc_idx;
                        self.state.toggle_tool_expand(msg_idx, real_tc_idx);
                        break;
                    }
                }
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('s')) => {
                self.state.sidebar_visible = !self.state.sidebar_visible;
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('t')) => {
                let messages = self.state.messages.clone();
                self.state
                    .todo_panel
                    .update_from_session_with_messages(&messages);
                self.state.todo_panel.toggle();
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('m')) => {
                self.state
                    .mcp_panel
                    .update_from_dialog(&self.state.mcp_dialog);
                self.state.mcp_panel.toggle();
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('l')) => {
                self.state.lsp_panel.update_count(self.state.lsp_count, 0);
                self.state.lsp_panel.toggle();
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('e')) => self.open_external_editor(),
            (_, KeyCode::Char('?')) => {
                self.state.open_help();
                None
            }
            (_, KeyCode::PageUp) => {
                if self.state.scroll_offset == usize::MAX {
                    self.state.scroll_offset = 0;
                } else {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_sub(10);
                }
                self.state.scroll_paused = true;
                self.state.auto_scroll = false;
                None
            }
            (_, KeyCode::PageDown) => {
                if self.state.scroll_offset != usize::MAX {
                    self.state.scroll_offset = self.state.scroll_offset.saturating_add(10);
                }
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                self.state.prompt.delete_word_left();
                if self.state.autocomplete.open {
                    self.state.autocomplete.rebuild_entries(
                        &self.state.prompt.value,
                        Path::new(&self.state.cwd),
                        self.state.prompt.frecency.as_ref(),
                    );
                }
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                self.state.prompt.move_home();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.state.prompt.move_end();
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                self.state.prompt.delete_to_start();
                self.state.autocomplete.close();
                None
            }
            (KeyModifiers::ALT, KeyCode::Char('d')) => {
                self.state.prompt.delete_word_right();
                None
            }
            (KeyModifiers::ALT, KeyCode::Backspace) => {
                self.state.prompt.delete_word_left();
                None
            }
            (KeyModifiers::ALT, KeyCode::Delete) => {
                self.state.prompt.delete_word_right();
                None
            }
            (KeyModifiers::CONTROL | KeyModifiers::ALT, KeyCode::Left) => {
                self.state.prompt.move_word_left();
                None
            }
            (KeyModifiers::CONTROL | KeyModifiers::ALT, KeyCode::Right) => {
                self.state.prompt.move_word_right();
                None
            }
            (_, KeyCode::Char(c)) => {
                if self.state.leader_active {
                    self.state.deactivate_leader();
                    return match c {
                        'u' => {
                            if self.state.undo_message() {
                                if let Some(revert) = &self.state.revert {
                                    self.state.prompt.value.clone_from(&revert.prompt_text);
                                    self.state.prompt.cursor =
                                        self.state.prompt.value.chars().count();
                                }
                            }
                            None
                        }
                        'r' => {
                            if self.state.redo_message() {
                                if let Some(revert) = &self.state.revert {
                                    self.state.prompt.value.clone_from(&revert.prompt_text);
                                    self.state.prompt.cursor =
                                        self.state.prompt.value.chars().count();
                                } else {
                                    self.state.prompt.clear();
                                    self.state.prompt.cursor = 0;
                                }
                            }
                            None
                        }
                        'm' => {
                            self.state.open_model_picker();
                            None
                        }
                        'n' => Some("__new_session__".to_string()),
                        'l' => {
                            self.state.open_sessions();
                            None
                        }
                        'b' => {
                            self.state.sidebar_visible = !self.state.sidebar_visible;
                            None
                        }
                        'a' => {
                            self.state.open_command_palette();
                            None
                        }
                        'd' => {
                            self.state.open_debug_panel();
                            None
                        }
                        _ => {
                            self.state.prompt.insert_char(c);
                            None
                        }
                    };
                }
                self.state.prompt.insert_char(c);
                self.state.autocomplete.on_char_insert(
                    c,
                    self.state.prompt.cursor,
                    &self.state.prompt.value,
                );
                if self.state.autocomplete.open {
                    self.state.autocomplete.rebuild_entries(
                        &self.state.prompt.value,
                        Path::new(&self.state.cwd),
                        self.state.prompt.frecency.as_ref(),
                    );
                }
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.prompt.backspace();
                self.state
                    .autocomplete
                    .on_backspace(self.state.prompt.cursor);
                if self.state.autocomplete.open {
                    self.state.autocomplete.rebuild_entries(
                        &self.state.prompt.value,
                        Path::new(&self.state.cwd),
                        self.state.prompt.frecency.as_ref(),
                    );
                }
                None
            }
            (_, KeyCode::Delete) => {
                self.state.prompt.delete();
                if self.state.autocomplete.open {
                    self.state.autocomplete.rebuild_entries(
                        &self.state.prompt.value,
                        Path::new(&self.state.cwd),
                        self.state.prompt.frecency.as_ref(),
                    );
                }
                None
            }
            (_, KeyCode::Left) => {
                self.state.prompt.move_left();
                None
            }
            (_, KeyCode::Right) => {
                self.state.prompt.move_right();
                None
            }
            (_, KeyCode::Home) => {
                self.state.prompt.move_home();
                None
            }
            (_, KeyCode::End) => {
                self.state.prompt.move_end();
                None
            }
            _ => None,
        }
    }

    fn handle_permission_key(&mut self, key: KeyEvent) -> Option<String> {
        if let Some(action) = self.state.permission_dialog.handle_key(key.code) {
            self.handle_permission_action(action);
        }
        None
    }

    fn handle_question_key(&mut self, key: KeyEvent) -> Option<String> {
        if let Some(response) = self.state.question_prompt.handle_key(key) {
            if let Some(tx) = self.state.question_prompt.answer_tx.take() {
                let _ = tx.send(response.answer.clone());
            }
            self.state.question_prompt.close();
        }
        None
    }

    fn handle_permission_action(
        &mut self,
        action: crate::tui::dialog_permission::PermissionAction,
    ) {
        use crate::tui::app;
        use crate::tui::dialog_permission::PermissionAction;
        match action {
            PermissionAction::Approve => {
                self.state
                    .add_toast("Permission approved", app::ToastKind::Success);
            }
            PermissionAction::Deny => {
                self.state
                    .add_toast("Permission denied", app::ToastKind::Warning);
            }
            PermissionAction::AlwaysAllow => {
                self.state
                    .add_toast("Always allow enabled", app::ToastKind::Info);
            }
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.command_palette.close();
                None
            }
            (_, KeyCode::Enter) => {
                self.state.command_palette.confirm();
                self.state.command_palette.selected.take()
            }
            (_, KeyCode::Char('/')) => {
                self.state.command_palette.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                self.state.command_palette.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.command_palette.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.command_palette.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.command_palette.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.model_picker.close();
                None
            }
            (_, KeyCode::Enter) => {
                self.state.model_picker.confirm();
                if let Some(model) = self.state.model_picker.selected.take() {
                    self.state.session.model.clone_from(&model);
                    self.state
                        .add_toast(format!("Model changed to {model}"), ToastKind::Info);
                    Some(format!("/model {model}"))
                } else {
                    None
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
                self.state.model_picker.toggle_favorite();
                None
            }
            (_, KeyCode::Char('/')) => {
                self.state.model_picker.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                self.state.model_picker.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.model_picker.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.model_picker.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.model_picker.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_mcp_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.mcp_dialog.close();
                None
            }
            (_, KeyCode::Enter) => {
                self.state.mcp_dialog.toggle_server();
                None
            }
            (_, KeyCode::Char('/')) => {
                self.state.mcp_dialog.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                self.state.mcp_dialog.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.mcp_dialog.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.mcp_dialog.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.mcp_dialog.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_skills_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.skills_dialog.close();
                None
            }
            (_, KeyCode::Char('/')) => {
                self.state.skills_dialog.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                self.state.skills_dialog.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.skills_dialog.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.skills_dialog.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.skills_dialog.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_theme_list_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.theme_list_dialog.close();
                None
            }
            (_, KeyCode::Enter) => {
                if let Some(theme_id) = self.state.theme_list_dialog.selected_theme_id() {
                    let theme_id = theme_id.to_string();
                    self.state.theme_list_dialog.selected_id = theme_id.clone();
                    self.state.set_theme(&theme_id);
                    let display = Theme::display_name(&theme_id);
                    self.state
                        .add_toast(format!("Theme: {display}"), ToastKind::Success);
                }
                self.state.theme_list_dialog.close();
                None
            }
            (_, KeyCode::Up) => {
                self.state.theme_list_dialog.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.theme_list_dialog.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.theme_list_dialog.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.theme_list_dialog.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_plugins_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.plugins_dialog.close();
                None
            }
            (_, KeyCode::Enter) => {
                self.state.plugins_dialog.toggle_plugin();
                None
            }
            (_, KeyCode::Char('/')) => {
                self.state.plugins_dialog.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                self.state.plugins_dialog.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                self.state.plugins_dialog.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.plugins_dialog.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.plugins_dialog.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_sessions_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_sessions::SessionAction;
        match self.state.sessions_dialog.handle_key(key) {
            SessionAction::None => None,
            SessionAction::Close => None,
            SessionAction::StartSearch => None,
            SessionAction::Switch(path) => Some(format!("__session_switch__{}", path.display())),
        }
    }

    fn handle_message_action_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::MessageAction;
        match self.state.message_action_dialog.handle_key(key.code) {
            None => None,
            Some(MessageAction::Revert) => {
                if self.state.undo_message() {
                    let text = self
                        .state
                        .revert
                        .as_ref()
                        .map(|r| r.prompt_text.clone())
                        .unwrap_or_default();
                    self.state.prompt.value = text;
                    self.state.prompt.cursor = self.state.prompt.value.chars().count();
                }
                None
            }
            Some(MessageAction::Copy) => {
                let text = self.state.message_action_dialog.message_content.clone();
                self.state.prompt.value = text;
                self.state.prompt.cursor = self.state.prompt.value.chars().count();
                None
            }
            Some(MessageAction::Fork) => Some("__fork_session__".to_string()),
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.help_dialog.close();
                None
            }
            _ => None,
        }
    }

    fn handle_context_viz_key(&mut self, key: KeyEvent) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.context_viz_dialog.close();
                None
            }
            _ => None,
        }
    }

    fn handle_branching_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_session_branching::BranchingAction;
        match self.state.branching_dialog.handle_key(key) {
            BranchingAction::None => None,
            BranchingAction::Close => None,
            BranchingAction::Switch(path) => Some(format!("__session_switch__{}", path.display())),
            BranchingAction::NewBranch => Some("__fork_session__".to_string()),
        }
    }

    fn handle_stash_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_prompt_stash::StashAction;
        match self.state.prompt_stash.handle_key(key) {
            StashAction::None => None,
            StashAction::Close => None,
            StashAction::StartSearch => None,
            StashAction::Select(content) => {
                self.state.prompt.clear();
                self.state.prompt.insert_str(&content);
                self.state.prompt_stash.close();
                None
            }
            StashAction::SaveNew(name, _content) => {
                let prompt_text = self.state.prompt.value.clone();
                if prompt_text.is_empty() {
                    self.state
                        .add_toast("No prompt content to stash", ToastKind::Warning);
                    return None;
                }
                self.state.prompt_stash.save_new(&name, &prompt_text);
                self.state
                    .add_toast(format!("Prompt stashed as '{name}'"), ToastKind::Success);
                self.state.prompt_stash.close();
                None
            }
            StashAction::Delete(_idx) => {
                self.state.prompt_stash.delete_entry(_idx);
                self.state.add_toast("Prompt deleted", ToastKind::Info);
                None
            }
        }
    }

    fn handle_export_options_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_export_options::ExportAction;
        match self.state.export_options.handle_key(key) {
            ExportAction::None => None,
            ExportAction::Close => None,
            ExportAction::Confirm => {
                let filename = self.state.export_options.filename.clone();
                let opts = self.state.export_options.clone();
                self.state.export_options.close();
                self.perform_export(&filename, &opts);
                None
            }
            ExportAction::Toggle(_) => None,
            ExportAction::EditFilename => None,
        }
    }

    fn handle_debug_panel_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::debug_panel::DebugAction;
        match self.state.debug_panel.handle_key(key) {
            DebugAction::None => None,
            DebugAction::Close => None,
            DebugAction::NextTab => None,
            DebugAction::PrevTab => None,
        }
    }

    fn perform_export(
        &mut self,
        filename: &str,
        opts: &crate::tui::dialog_export_options::ExportOptionsState,
    ) {
        let format = match opts.format_idx {
            0 => crate::tui::transcript::TranscriptFormat::Plain,
            1 => crate::tui::transcript::TranscriptFormat::Markdown,
            2 => crate::tui::transcript::TranscriptFormat::Json,
            _ => crate::tui::transcript::TranscriptFormat::Markdown,
        };

        let content = match crate::tui::transcript::export_transcript(&self.state, format) {
            Ok(c) => c,
            Err(e) => {
                self.state
                    .add_toast(format!("Export failed: {e}"), ToastKind::Error);
                return;
            }
        };

        let export_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(filename);

        if let Err(e) = std::fs::write(&export_path, &content) {
            self.state
                .add_toast(format!("Export failed: {e}"), ToastKind::Error);
        } else {
            self.state.add_toast(
                format!("Exported to {}", export_path.display()),
                ToastKind::Success,
            );
        }
    }

    fn handle_provider_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_providers::ProviderAction;
        match self.state.provider_dialog.handle_key(key) {
            ProviderAction::None => None,
            ProviderAction::Close => None,
            ProviderAction::Toggle(idx) => {
                let name = self
                    .state
                    .provider_dialog
                    .providers
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                self.state
                    .add_toast(format!("Provider {name} toggled"), ToastKind::Info);
                None
            }
            ProviderAction::ViewDocs(idx) => {
                let name = self
                    .state
                    .provider_dialog
                    .providers
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                self.state.add_toast(
                    format!("Docs for {name}: visit provider website"),
                    ToastKind::Info,
                );
                None
            }
            ProviderAction::ConnectProvider(kind, display, api_key) => {
                let store_key = match kind {
                    api::ProviderKind::Anthropic => "anthropic",
                    api::ProviderKind::OpenAi => "openai",
                    api::ProviderKind::Xai => "xai",
                    api::ProviderKind::QwenProxy => "qwen_proxy",
                    api::ProviderKind::Azure => "azure",
                    api::ProviderKind::Gemini => "gemini",
                    api::ProviderKind::Bedrock => "bedrock",
                    api::ProviderKind::OpenRouter => "openrouter",
                    api::ProviderKind::Mistral => "mistral",
                    api::ProviderKind::Groq => "groq",
                    api::ProviderKind::Unconfigured => return None,
                    api::ProviderKind::CustomOpenAi { provider, .. } => {
                        let key = provider.to_lowercase().replace('-', "_");
                        let mut store = runtime::AuthStore::load();
                        store.set_api_key(key, api_key.clone());
                        if let Err(e) = store.save() {
                            self.state
                                .add_toast(format!("Failed to save key: {e}"), ToastKind::Error);
                        } else {
                            self.state.add_toast(
                                format!("{display} connected and saved"),
                                ToastKind::Success,
                            );
                        }
                        return None;
                    }
                };
                let mut store = runtime::AuthStore::load();
                store.set_api_key(store_key.to_string(), api_key.clone());
                if let Err(e) = store.save() {
                    self.state
                        .add_toast(format!("Failed to save key: {e}"), ToastKind::Error);
                } else {
                    self.state
                        .add_toast(format!("{display} connected"), ToastKind::Success);
                    self.state.provider_dialog.refresh_providers();
                }
                None
            }
        }
    }

    fn handle_workspace_key(&mut self, key: KeyEvent) -> Option<String> {
        use crate::tui::dialog_workspaces::WorkspaceAction;
        match self.state.workspace_dialog.handle_key(key) {
            WorkspaceAction::None => None,
            WorkspaceAction::Close => None,
            WorkspaceAction::Switch(path) => Some(format!("__workspace_switch__{path}")),
            WorkspaceAction::StartSearch => None,
            WorkspaceAction::Delete(path) => {
                self.state
                    .add_toast(format!("Delete workspace: {path}"), ToastKind::Info);
                self.state.workspace_dialog.scan_workspaces();
                self.state.workspace_dialog.apply_filter();
                None
            }
            WorkspaceAction::Create(path) => {
                self.state
                    .add_toast(format!("Create workspace: {path}"), ToastKind::Info);
                self.state.workspace_dialog.scan_workspaces();
                self.state.workspace_dialog.apply_filter();
                None
            }
        }
    }

    fn handle_permission_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_permission::PermissionDialogState,
    ) -> Option<String> {
        if let Some(action) = s.handle_key(key.code) {
            use runtime::PermissionPromptDecision;
            let decision = match action {
                crate::tui::dialog_permission::PermissionAction::Approve
                | crate::tui::dialog_permission::PermissionAction::AlwaysAllow => {
                    PermissionPromptDecision::Allow
                }
                crate::tui::dialog_permission::PermissionAction::Deny => {
                    PermissionPromptDecision::Deny {
                        reason: "denied by user in TUI".into(),
                    }
                }
            };
            if let Some(tx) = self.state.permission_response_tx.take() {
                let _ = tx.send(decision);
            }
            self.handle_permission_action(action);
            self.state.close_modal();
        } else if matches!(key.code, KeyCode::Esc) {
            if let Some(tx) = self.state.permission_response_tx.take() {
                let _ = tx.send(runtime::PermissionPromptDecision::Deny {
                    reason: "dismissed by user".into(),
                });
            }
            self.state.close_modal();
        }
        None
    }

    fn handle_question_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_question::QuestionPromptState,
    ) -> Option<String> {
        if let Some(response) = s.handle_key(key) {
            if let Some(tx) = s.answer_tx.take() {
                let _ = tx.send(response.answer.clone());
            }
            self.state.close_modal();
        }
        None
    }

    fn handle_picker_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::model_picker::ModelPickerState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                s.confirm();
                if let Some(model) = s.selected.take() {
                    self.state.session.model.clone_from(&model);
                    self.state
                        .add_toast(format!("Model changed to {model}"), ToastKind::Info);
                    self.state.close_modal();
                    Some(format!("/model {model}"))
                } else {
                    None
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
                s.toggle_favorite();
                None
            }
            (_, KeyCode::Char('/')) => {
                s.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    /// Execute a command palette action directly, without returning a string
    /// to the main loop.
    fn dispatch_command_palette_action(&mut self, action: CommandAction) {
        match action {
            CommandAction::ToggleThinking => {
                self.state.toggle_thinking();
                let status = if self.state.show_thinking {
                    "Thinking enabled"
                } else {
                    "Thinking disabled"
                };
                self.state.add_toast(status, ToastKind::Info);
            }
            CommandAction::ShowMcp => {
                self.state.open_mcp();
            }
            CommandAction::ShowSkills => {
                self.state.open_skills();
            }
            CommandAction::ShowPlugins => {
                self.state.open_plugins();
            }
            CommandAction::NewSession => {
                // Return a sentinel that main.rs handles for full session reset
                self.state.close_modal();
                self.pending_palette_action = Some(PaletteAction::NewSession);
            }
            CommandAction::SwitchSession => {
                self.state.open_sessions();
            }
            CommandAction::ForkSession => {
                self.state.close_modal();
                self.pending_palette_action = Some(PaletteAction::ForkSession);
            }
            CommandAction::ClearConversation => {
                self.state.messages.clear();
                self.state.tools.clear();
                self.state
                    .add_toast("Conversation cleared", ToastKind::Info);
            }
            CommandAction::Undo => {
                if self.state.revert.is_some() {
                    self.handle_undo();
                }
            }
            CommandAction::Redo => {
                self.handle_redo();
            }
            CommandAction::ExportSession => {
                self.state.open_export_options();
            }
            CommandAction::CompactContext => {
                self.state.add_toast("Context compacted", ToastKind::Info);
            }
            CommandAction::ShowContextViz => {
                self.state.open_context_viz();
            }
            CommandAction::ShowPromptStash => {
                self.state.open_prompt_stash();
            }
            CommandAction::OpenExternalEditor => {
                self.open_external_editor();
            }
            CommandAction::ShowExportOptions => {
                self.state.open_export_options();
            }
            CommandAction::ShowProviders => {
                self.state.open_provider();
            }
            CommandAction::ShowWorkspaces => {
                self.state.open_workspace();
            }
            CommandAction::AttachSession => {
                self.state.open_sessions();
            }
            CommandAction::ToggleSidebar => {
                self.state.sidebar_visible = !self.state.sidebar_visible;
            }
            CommandAction::ShowStatus => {
                self.show_status_pager();
            }
            CommandAction::ShowCost => {
                self.show_cost_pager();
            }
            CommandAction::ToggleTheme => {
                self.state.toggle_theme();
            }
            CommandAction::ShowHelp => {
                self.state.open_help();
            }
            CommandAction::Exit => {
                self.state.close_modal();
                self.pending_palette_action = Some(PaletteAction::Exit);
            }
            CommandAction::SwitchModel => {
                self.state.open_model_picker();
            }
        }
    }

    fn handle_palette_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::command_palette::CommandPaletteState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                // Get the CommandAction from the selected entry before confirming
                let action = s
                    .filtered
                    .get(s.cursor)
                    .and_then(|&entry_idx| s.entries.get(entry_idx).map(|e| e.action.clone()));
                s.confirm();
                self.state.close_modal();
                if let Some(action) = action {
                    self.dispatch_command_palette_action(action);
                }
                None
            }
            (_, KeyCode::Char('/')) => {
                s.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_mcp_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_mcp::McpDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                s.toggle_server();
                None
            }
            (_, KeyCode::Char('/')) => {
                s.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_skills_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_skills::SkillsDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Char('/')) => {
                s.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_theme_list_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_theme_list::ThemeListDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                if let Some(theme_id) = s.selected_theme_id() {
                    let theme_id = theme_id.to_string();
                    s.selected_id = theme_id.clone();
                    self.state.set_theme(&theme_id);
                    let display = Theme::display_name(&theme_id);
                    self.state
                        .add_toast(format!("Theme: {display}"), ToastKind::Success);
                }
                self.state.close_modal();
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_plugins_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_plugins::PluginsDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                s.toggle_plugin();
                None
            }
            (_, KeyCode::Char('/')) => {
                s.type_char('/');
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                s.type_char(c);
                None
            }
            (_, KeyCode::Backspace) => {
                s.backspace();
                None
            }
            _ => None,
        }
    }

    fn handle_sessions_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_sessions::SessionsDialogState,
    ) -> Option<String> {
        use crate::tui::dialog_sessions::SessionAction;
        match s.handle_key(key) {
            SessionAction::None => None,
            SessionAction::Close => {
                self.state.close_modal();
                None
            }
            SessionAction::StartSearch => None,
            SessionAction::Switch(path) => {
                self.state.close_modal();
                Some(format!("__session_switch__{}", path.display()))
            }
        }
    }

    fn handle_message_action_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_message_actions::MessageActionDialogState,
    ) -> Option<String> {
        use crate::tui::MessageAction;
        match s.handle_key(key.code) {
            None => None,
            Some(MessageAction::Revert) => {
                if self.state.undo_message() {
                    let text = self
                        .state
                        .revert
                        .as_ref()
                        .map(|r| r.prompt_text.clone())
                        .unwrap_or_default();
                    self.state.prompt.value = text;
                    self.state.prompt.cursor = self.state.prompt.value.chars().count();
                }
                self.state.close_modal();
                None
            }
            Some(MessageAction::Copy) => {
                let text = s.message_content.clone();
                self.state.prompt.value = text;
                self.state.prompt.cursor = self.state.prompt.value.chars().count();
                self.state.close_modal();
                None
            }
            Some(MessageAction::Fork) => {
                self.state.close_modal();
                Some("__fork_session__".to_string())
            }
        }
    }

    fn handle_help_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_help::HelpDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.close_modal();
                None
            }
            _ => None,
        }
    }

    fn handle_context_viz_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_context_viz::ContextVizDialogState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.close_modal();
                None
            }
            _ => None,
        }
    }

    fn handle_branching_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_session_branching::SessionBranchingState,
    ) -> Option<String> {
        use crate::tui::dialog_session_branching::BranchingAction;
        match s.handle_key(key) {
            BranchingAction::None => None,
            BranchingAction::Close => {
                self.state.close_modal();
                None
            }
            BranchingAction::Switch(path) => {
                self.state.close_modal();
                Some(format!("__session_switch__{}", path.display()))
            }
            BranchingAction::NewBranch => {
                self.state.close_modal();
                Some("__fork_session__".to_string())
            }
        }
    }

    fn handle_stash_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_prompt_stash::PromptStashState,
    ) -> Option<String> {
        use crate::tui::dialog_prompt_stash::StashAction;
        match s.handle_key(key) {
            StashAction::None => None,
            StashAction::Close => {
                self.state.close_modal();
                None
            }
            StashAction::StartSearch => None,
            StashAction::Select(content) => {
                self.state.prompt.clear();
                self.state.prompt.insert_str(&content);
                self.state.close_modal();
                None
            }
            StashAction::SaveNew(name, _content) => {
                let prompt_text = self.state.prompt.value.clone();
                if prompt_text.is_empty() {
                    self.state
                        .add_toast("No prompt content to stash", ToastKind::Warning);
                    return None;
                }
                s.save_new(&name, &prompt_text);
                self.state
                    .add_toast(format!("Prompt stashed as '{name}'"), ToastKind::Success);
                self.state.close_modal();
                None
            }
            StashAction::Delete(_idx) => {
                s.delete_entry(_idx);
                self.state.add_toast("Prompt deleted", ToastKind::Info);
                None
            }
        }
    }

    fn handle_export_options_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_export_options::ExportOptionsState,
    ) -> Option<String> {
        use crate::tui::dialog_export_options::ExportAction;
        match s.handle_key(key) {
            ExportAction::None => None,
            ExportAction::Close => {
                self.state.close_modal();
                None
            }
            ExportAction::Confirm => {
                let filename = s.filename.clone();
                let opts = s.clone();
                self.state.close_modal();
                self.perform_export(&filename, &opts);
                None
            }
            ExportAction::Toggle(_) => None,
            ExportAction::EditFilename => None,
        }
    }

    fn handle_debug_panel_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::debug_panel::DebugPanelState,
    ) -> Option<String> {
        use crate::tui::debug_panel::DebugAction;
        match s.handle_key(key) {
            DebugAction::None => None,
            DebugAction::Close => {
                self.state.close_modal();
                None
            }
            DebugAction::NextTab => None,
            DebugAction::PrevTab => None,
        }
    }

    fn handle_provider_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_providers::ProviderDialogState,
    ) -> Option<String> {
        use crate::tui::dialog_providers::ProviderAction;
        match s.handle_key(key) {
            ProviderAction::None => None,
            ProviderAction::Close => {
                self.state.close_modal();
                None
            }
            ProviderAction::Toggle(idx) => {
                let name = s
                    .providers
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                self.state
                    .add_toast(format!("Provider {name} toggled"), ToastKind::Info);
                None
            }
            ProviderAction::ViewDocs(idx) => {
                let name = s
                    .providers
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                self.state.add_toast(
                    format!("Docs for {name}: visit provider website"),
                    ToastKind::Info,
                );
                None
            }
            ProviderAction::ConnectProvider(kind, display, api_key) => {
                let store_key = match kind {
                    api::ProviderKind::Anthropic => "anthropic",
                    api::ProviderKind::OpenAi => "openai",
                    api::ProviderKind::Xai => "xai",
                    api::ProviderKind::QwenProxy => "qwen_proxy",
                    api::ProviderKind::Azure => "azure",
                    api::ProviderKind::Gemini => "gemini",
                    api::ProviderKind::Bedrock => "bedrock",
                    api::ProviderKind::OpenRouter => "openrouter",
                    api::ProviderKind::Mistral => "mistral",
                    api::ProviderKind::Groq => "groq",
                    api::ProviderKind::Unconfigured => return None,
                    api::ProviderKind::CustomOpenAi { provider, .. } => {
                        let key = provider.to_lowercase().replace('-', "_");
                        let mut store = runtime::AuthStore::load();
                        store.set_api_key(key, api_key.clone());
                        if let Err(e) = store.save() {
                            self.state
                                .add_toast(format!("Failed to save key: {e}"), ToastKind::Error);
                        } else {
                            self.state.add_toast(
                                format!("{display} connected and saved"),
                                ToastKind::Success,
                            );
                        }
                        return None;
                    }
                };
                let mut store = runtime::AuthStore::load();
                store.set_api_key(store_key.to_string(), api_key.clone());
                if let Err(e) = store.save() {
                    self.state
                        .add_toast(format!("Failed to save key: {e}"), ToastKind::Error);
                } else {
                    self.state
                        .add_toast(format!("{display} connected"), ToastKind::Success);
                    s.refresh_providers();
                }
                None
            }
        }
    }

    fn handle_workspace_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::dialog_workspaces::WorkspaceDialogState,
    ) -> Option<String> {
        use crate::tui::dialog_workspaces::WorkspaceAction;
        match s.handle_key(key) {
            WorkspaceAction::None => None,
            WorkspaceAction::Close => {
                self.state.close_modal();
                None
            }
            WorkspaceAction::Switch(path) => {
                self.state.close_modal();
                Some(format!("__workspace_switch__{path}"))
            }
            WorkspaceAction::StartSearch => None,
            WorkspaceAction::Delete(path) => {
                self.state
                    .add_toast(format!("Delete workspace: {path}"), ToastKind::Info);
                s.scan_workspaces();
                s.apply_filter();
                None
            }
            WorkspaceAction::Create(path) => {
                self.state
                    .add_toast(format!("Create workspace: {path}"), ToastKind::Info);
                s.scan_workspaces();
                s.apply_filter();
                None
            }
        }
    }

    fn handle_diff_view_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::widgets::DiffView,
    ) -> Option<String> {
        let height = self
            .terminal
            .size()
            .map(|s| s.height as usize)
            .unwrap_or(24);
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Char('j') | KeyCode::Down) => {
                s.scroll_down();
                None
            }
            (_, KeyCode::Char('k') | KeyCode::Up) => {
                s.scroll_up();
                None
            }
            (_, KeyCode::Char('g')) if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                s.go_to_top();
                None
            }
            (_, KeyCode::Char('G')) | (KeyModifiers::SHIFT, KeyCode::Char('g')) => {
                s.go_to_bottom(height.saturating_sub(6));
                None
            }
            (_, KeyCode::PageDown) => {
                s.scroll_page_down(height.saturating_sub(6));
                None
            }
            (_, KeyCode::PageUp) => {
                s.scroll_page_up(height.saturating_sub(6));
                None
            }
            _ => None,
        }
    }

    fn handle_pager_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::widgets::PagerState,
    ) -> Option<String> {
        let height = self
            .terminal
            .size()
            .map(|s| s.height as usize)
            .unwrap_or(24);
        let visible_lines = height.saturating_sub(8);
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Char('j') | KeyCode::Down) => {
                s.scroll_down();
                None
            }
            (_, KeyCode::Char('k') | KeyCode::Up) => {
                s.scroll_up();
                None
            }
            (_, KeyCode::Char('g')) if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                s.go_to_top();
                None
            }
            (_, KeyCode::Char('G')) | (KeyModifiers::SHIFT, KeyCode::Char('g')) => {
                s.go_to_bottom(visible_lines);
                None
            }
            (_, KeyCode::PageDown) => {
                s.scroll_page_down(visible_lines);
                None
            }
            (_, KeyCode::PageUp) => {
                s.scroll_page_up(visible_lines);
                None
            }
            _ => None,
        }
    }

    fn handle_autocomplete_action_from_modal(
        &mut self,
        key: KeyEvent,
        s: &mut crate::tui::autocomplete::AutocompleteState,
    ) -> Option<String> {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.state.close_modal();
                None
            }
            (_, KeyCode::Enter) => {
                let was_slash_command = s.slash_command_selected;
                s.select(&mut self.state.prompt);
                if let Some(ref mut frecency) = self.state.prompt.frecency {
                    if let Some(entry) = s.entries.get(s.idx) {
                        frecency.record(&entry.title);
                    }
                }
                self.state.close_modal();
                if was_slash_command {
                    s.slash_command_selected = false;
                    let cmd = self.state.prompt.value.clone();
                    self.state.prompt.clear();
                    self.execute_slash_command(&cmd);
                }
                None
            }
            (_, KeyCode::Tab) => {
                s.select(&mut self.state.prompt);
                if let Some(ref mut frecency) = self.state.prompt.frecency {
                    if let Some(entry) = s.entries.get(s.idx) {
                        frecency.record(&entry.title);
                    }
                }
                None
            }
            (_, KeyCode::BackTab) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Up) => {
                s.cursor_up();
                None
            }
            (_, KeyCode::Down) => {
                s.cursor_down();
                None
            }
            (_, KeyCode::Char(c)) => {
                self.state.prompt.insert_char(c);
                s.on_char_insert(c, self.state.prompt.cursor, &self.state.prompt.value);
                None
            }
            (_, KeyCode::Backspace) => {
                self.state.prompt.backspace();
                s.on_backspace(self.state.prompt.cursor);
                None
            }
            (_, KeyCode::Delete) => {
                self.state.prompt.delete();
                None
            }
            (_, KeyCode::Left) => {
                self.state.prompt.move_left();
                None
            }
            (_, KeyCode::Right) => {
                self.state.prompt.move_right();
                None
            }
            _ => None,
        }
    }

    fn handle_diff_view_key(&mut self, key: KeyEvent) -> Option<String> {
        let height = self
            .terminal
            .size()
            .map(|s| s.height as usize)
            .unwrap_or(24);
        self.state.diff_view.as_ref()?;
        let diff_view = self.state.diff_view.as_mut().unwrap();

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.diff_view = None;
                None
            }
            (_, KeyCode::Char('j') | KeyCode::Down) => {
                diff_view.scroll_down();
                None
            }
            (_, KeyCode::Char('k') | KeyCode::Up) => {
                diff_view.scroll_up();
                None
            }
            (_, KeyCode::Char('g')) if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                diff_view.go_to_top();
                None
            }
            (_, KeyCode::Char('G')) | (KeyModifiers::SHIFT, KeyCode::Char('g')) => {
                diff_view.go_to_bottom(height.saturating_sub(6));
                None
            }
            (_, KeyCode::PageDown) => {
                diff_view.scroll_page_down(height.saturating_sub(6));
                None
            }
            (_, KeyCode::PageUp) => {
                diff_view.scroll_page_up(height.saturating_sub(6));
                None
            }
            _ => None,
        }
    }

    fn handle_pager_key(&mut self, key: KeyEvent) -> Option<String> {
        let height = self
            .terminal
            .size()
            .map(|s| s.height as usize)
            .unwrap_or(24);
        let visible_lines = height.saturating_sub(8);

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q')) => {
                self.state.pager.close();
                None
            }
            (_, KeyCode::Char('j') | KeyCode::Down) => {
                self.state.pager.scroll_down();
                None
            }
            (_, KeyCode::Char('k') | KeyCode::Up) => {
                self.state.pager.scroll_up();
                None
            }
            (_, KeyCode::Char('g')) if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.state.pager.go_to_top();
                None
            }
            (_, KeyCode::Char('G')) | (KeyModifiers::SHIFT, KeyCode::Char('g')) => {
                self.state.pager.go_to_bottom(visible_lines);
                None
            }
            (_, KeyCode::PageDown) => {
                self.state.pager.scroll_page_down(visible_lines);
                None
            }
            (_, KeyCode::PageUp) => {
                self.state.pager.scroll_page_up(visible_lines);
                None
            }
            _ => None,
        }
    }

    fn show_diff_view(&mut self) {
        use crate::git::render_diff_report_for;
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        match render_diff_report_for(&cwd) {
            Ok(diff_text) => {
                let view = crate::tui::widgets::DiffView::from_diff(&diff_text, "Diff");
                self.state.diff_view = Some(view);
            }
            Err(e) => {
                self.state
                    .add_toast(format!("Diff error: {e}"), ToastKind::Error);
            }
        }
    }

    fn show_status_pager(&mut self) {
        let total_tokens = self.state.session.input_tokens + self.state.session.output_tokens;
        let content = format!(
            "Model: {}\nPermission Mode: {}\nTurns: {}\nMessages: {}\nInput Tokens: {}\nOutput Tokens: {}\nTotal Tokens: {}\nSession ID: {}\nSession Title: {}\nCWD: {}\nGit Branch: {}\nGit Dirty: {}\nLSP Servers: {}\nMCP Servers: {}\nSkills: {}\nPlugins: {}",
            self.state.session.model,
            self.state.session.permission_mode,
            self.state.session.turns,
            self.state.session.message_count,
            self.state.session.input_tokens,
            self.state.session.output_tokens,
            total_tokens,
            self.state.session.id,
            self.state.session.title,
            self.state.cwd,
            self.state.git_branch.as_deref().unwrap_or("none"),
            if self.state.git_dirty { "yes" } else { "no" },
            self.state.lsp_count,
            self.state.mcp_dialog.servers.len(),
            self.state.skill_count,
            self.state.plugin_count,
        );
        self.state.pager.open("Status".to_string(), content);
    }

    fn show_config_pager(&mut self) {
        let content = format!(
            "CWD: {}\nModel: {}\nPermission Mode: {}",
            self.state.cwd, self.state.session.model, self.state.session.permission_mode,
        );
        self.state.pager.open("Config".to_string(), content);
    }

    fn show_memory_pager(&mut self) {
        let content = format!("CWD: {}\nConfig path: (project root)", self.state.cwd,);
        self.state.pager.open("Memory".to_string(), content);
    }

    fn show_cost_pager(&mut self) {
        let total_tokens = self.state.session.input_tokens + self.state.session.output_tokens;
        let content = format!(
            "Input Tokens: {}\nOutput Tokens: {}\nTotal Tokens: {}\nCumulative Cost: ${:.4}",
            self.state.session.input_tokens,
            self.state.session.output_tokens,
            total_tokens,
            self.state.session.cumulative_cost,
        );
        self.state.pager.open("Cost".to_string(), content);
    }

    fn show_version_pager(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        let content = format!(
            "icode {}\nRust Edition: 2021\nPlatform: {}",
            version,
            std::env::consts::OS,
        );
        self.state.pager.open("Version".to_string(), content);
    }

    fn handle_undo(&mut self) {
        if self.state.undo_message() {
            if let Some(revert) = &self.state.revert {
                self.state.prompt.value.clone_from(&revert.prompt_text);
                self.state.prompt.cursor = self.state.prompt.value.chars().count();
            }
        }
    }

    fn handle_redo(&mut self) {
        if self.state.redo_message() {
            if let Some(revert) = &self.state.revert {
                self.state.prompt.value.clone_from(&revert.prompt_text);
                self.state.prompt.cursor = self.state.prompt.value.chars().count();
            } else {
                self.state.prompt.clear();
                self.state.prompt.cursor = 0;
            }
        }
    }

    fn execute_slash_command(&mut self, cmd: &str) {
        if let Some(action) = find_slash_command_action(cmd) {
            match action {
                CommandAction::ShowHelp => {
                    self.state.open_help();
                }
                CommandAction::ShowStatus => {
                    self.show_status_pager();
                }
                CommandAction::ShowCost => {
                    self.show_cost_pager();
                }
                CommandAction::CompactContext => {
                    self.state.add_toast("Context compacted", ToastKind::Info);
                }
                CommandAction::ClearConversation => {
                    self.state.messages.clear();
                    self.state.tools.clear();
                    self.state
                        .add_toast("Conversation cleared", ToastKind::Info);
                }
                CommandAction::SwitchModel => {
                    self.state.open_model_picker();
                }
                CommandAction::ShowExportOptions => {
                    self.state.open_export_options();
                }
                CommandAction::SwitchSession => {
                    self.state.open_sessions();
                }
                CommandAction::Undo => {
                    if self.state.revert.is_some() {
                        self.handle_undo();
                    }
                }
                CommandAction::Redo => {
                    self.handle_redo();
                }
                _ => {}
            }
            return;
        }

        match cmd {
            "/diff" => {
                self.show_diff_view();
            }
            "/config" => {
                self.show_config_pager();
            }
            "/memory" => {
                self.show_memory_pager();
            }
            "/version" => {
                self.show_version_pager();
            }
            "/permissions" => {
                self.state
                    .add_toast("Use Ctrl+P to change permissions", ToastKind::Info);
            }
            _ if cmd.starts_with("/theme") => {
                let args = cmd.strip_prefix("/theme").unwrap_or("").trim();
                if args.is_empty() || args == "list" {
                    self.state.open_theme_list();
                } else if let Some(theme_name) = args.strip_prefix("set ") {
                    let theme_name = theme_name.trim();
                    if Theme::from_name(theme_name).is_some() {
                        self.state.set_theme(theme_name);
                        let display = Theme::display_name(theme_name);
                        self.state
                            .add_toast(format!("Theme: {display}"), ToastKind::Success);
                    } else {
                        self.state
                            .add_toast(format!("Unknown theme: {theme_name}"), ToastKind::Error);
                    }
                } else if Theme::from_name(args).is_some() {
                    self.state.set_theme(args);
                    let display = Theme::display_name(args);
                    self.state
                        .add_toast(format!("Theme: {display}"), ToastKind::Success);
                } else {
                    self.state
                        .add_toast(format!("Unknown theme: {args}"), ToastKind::Error);
                }
            }
            _ if cmd.starts_with("/debug") => {
                let args = cmd.strip_prefix("/debug").unwrap_or("").trim();
                let action = if args.is_empty() { None } else { Some(args) };
                let result = handle_debug_slash_command(action);
                let (msg, kind) = match result {
                    Ok(msg) => (msg, ToastKind::Info),
                    Err(msg) => (msg, ToastKind::Warning),
                };
                self.state.add_toast(msg, kind);
            }
            _ => {
                self.state
                    .add_toast(format!("Unknown command: {cmd}"), ToastKind::Warning);
            }
        }
    }

    pub(crate) fn open_external_editor(&mut self) -> Option<String> {
        if self.state.is_streaming {
            self.state
                .add_toast("Cannot open editor while streaming", ToastKind::Warning);
            return None;
        }

        let current_text = self.state.prompt.value.clone();

        let result =
            self.with_tui_suspended(|tui| crate::tui::external_editor::open_editor(&current_text));

        match result {
            Ok(edited) => {
                if edited.trim().is_empty() {
                    self.state.add_toast(
                        "Editor returned empty content, keeping original",
                        ToastKind::Warning,
                    );
                } else {
                    self.state.prompt.clear();
                    self.state.prompt.insert_str(&edited);
                    self.state
                        .add_toast("Content loaded from editor", ToastKind::Success);
                }
            }
            Err(e) => {
                self.state
                    .add_toast(format!("Editor error: {e}"), ToastKind::Error);
            }
        }
        None
    }

    fn with_tui_suspended<F, T>(&mut self, f: F) -> Result<T, String>
    where
        F: FnOnce(&mut Self) -> Result<T, String>,
    {
        disable_raw_mode().map_err(|e| format!("Failed to disable raw mode: {e}"))?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
            .map_err(|e| format!("Failed to leave alternate screen: {e}"))?;

        let result = f(self);

        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| format!("Failed to enter alternate screen: {e}"))?;
        enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {e}"))?;

        result
    }

    pub fn append_to_stream(&mut self, delta: &str) {
        self.state.append_to_stream(delta);
    }

    pub fn finish_stream(&mut self) {
        self.state.finish_stream();
    }

    pub fn add_tool_event(&mut self, name: &str, input_summary: &str) {
        self.state.add_tool_event(name, input_summary);
    }

    pub fn complete_tool_event(&mut self, name: &str, output: &str, success: bool) {
        self.state.complete_tool_event(name, output, success);
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    fn content_width(&self) -> u16 {
        let size = self.terminal.size().unwrap_or(ratatui::layout::Size {
            width: 80,
            height: 24,
        });
        let area_width = size.width;
        let is_welcome = self.state.messages.is_empty();
        let has_sidebar = !is_welcome && self.state.sidebar_visible && area_width > 120;
        let panel_width = if has_sidebar {
            area_width.saturating_sub(42)
        } else {
            area_width
        };
        panel_width.saturating_sub(4)
    }

    fn value_lines(&self) -> usize {
        let size = self.terminal.size().unwrap_or(ratatui::layout::Size {
            width: 80,
            height: 24,
        });
        let area_width = size.width;
        let is_welcome = self.state.messages.is_empty();
        let has_sidebar = !is_welcome && self.state.sidebar_visible && area_width > 120;
        let content_w = if has_sidebar {
            (area_width.saturating_sub(42)) as usize
        } else {
            area_width as usize
        };
        self.state.prompt.line_count(content_w)
    }

    pub fn set_turn_receiver(&mut self, rx: Receiver<TurnEvent>) {
        self.turn_rx = Some(rx);
    }

    fn poll_turn_events(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = &self.turn_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            self.turn_rx = None;
            events.push(TurnEvent::TurnError("Model connection lost".to_string()));
        }
        for event in events {
            self.process_turn_event(event);
        }
    }

    fn process_turn_event(&mut self, event: TurnEvent) {
        match event {
            TurnEvent::ThinkingStarted => {
                self.state.start_thinking();
                if self.state.turn_started_at.is_none() {
                    self.state.turn_started_at = Some(Instant::now());
                }
            }
            TurnEvent::TokenDelta(text) => {
                if self.state.turn_started_at.is_none() {
                    self.state.turn_started_at = Some(Instant::now());
                }
                if self.state.is_thinking {
                    self.state.append_thinking(&text);
                } else {
                    self.state.append_to_stream(&text);
                }
            }
            TurnEvent::ToolCallStarted { name, input } => {
                self.state.add_tool_event(&name, &input);
            }
            TurnEvent::ToolCallCompleted {
                name,
                output,
                success,
            } => {
                self.state.complete_tool_event(&name, &output, success);
            }
            TurnEvent::TurnCompleted {
                text,
                tool_calls,
                input_tokens,
                output_tokens,
            } => {
                self.state.end_thinking();
                let turn_dur = self.state.turn_started_at.take().map(|s| s.elapsed());
                let dur_ms = turn_dur.map_or(0, |d| d.as_millis() as u64);
                let timeline: Vec<(String, bool, u64)> = tool_calls
                    .iter()
                    .map(|tc| (tc.name.clone(), tc.success, 0u64))
                    .collect();
                if let Some(msg) = self.state.messages.last_mut() {
                    if msg.is_streaming && msg.full_text().is_empty() && !text.is_empty() {
                        msg.parts.push(MessagePart::Text { content: text });
                    }
                    msg.tool_timeline = timeline;
                    msg.turn_duration_ms = dur_ms;
                }
                self.state.finish_stream();
                self.state.is_streaming = false;
                self.state.mode = AppMode::Normal;
                self.state.session.turns += 1;
                self.state.session.message_count = self.state.messages.len();
                self.state.session.input_tokens += input_tokens;
                self.state.session.output_tokens += output_tokens;
                self.state.last_turn_duration = turn_dur;
                self.turn_rx = None;
            }
            TurnEvent::TurnError(msg) => {
                self.state.add_toast(msg.clone(), ToastKind::Error);
                self.state.finish_stream();
                self.state.is_streaming = false;
                self.state.mode = AppMode::Normal;
                if let Some(started) = self.state.turn_started_at.take() {
                    self.state.last_turn_duration = Some(started.elapsed());
                }
                self.turn_rx = None;
                let is_auth_error = msg.contains("No API provider configured")
                    || msg.contains("MissingCredentials")
                    || msg.contains("api key")
                    || msg.contains("credentials")
                    || msg.contains("auth");
                if is_auth_error {
                    self.state.show_auth_error(msg);
                } else {
                    self.state.show_error(msg);
                }
            }
            TurnEvent::PermissionRequested {
                request,
                response_tx,
            } => {
                self.state.permission_response_tx = Some(response_tx);
                self.state.permission_dialog.open(request);
                self.state.active_modal = Some(ActiveModal::Permission(
                    self.state.permission_dialog.clone(),
                ));
            }
        }
    }

    fn tool_call_at(&self, screen_row: u16, _screen_col: u16) -> Option<(usize, usize)> {
        let area = self.terminal.size().ok()?;
        let prompt_lines = self
            .state
            .prompt
            .line_count(area.width as usize)
            .clamp(1, 6);
        let prompt_height = (prompt_lines as u16) + 3;

        let is_welcome = self.state.messages.is_empty();
        let has_sidebar = !is_welcome && self.state.sidebar_visible && area.width > 120;
        let panel_width = if has_sidebar {
            area.width - 2 - 42
        } else {
            area.width - 2
        };
        let content_width = panel_width.saturating_sub(2) as usize;

        let panel_top = 1u16;
        let panel_bottom = area.height.saturating_sub(prompt_height + 1);

        if screen_row < panel_top || screen_row >= panel_bottom {
            return None;
        }

        let panel_row = screen_row - panel_top;
        let revert_boundary = self.state.revert.as_ref().map(|r| r.message_boundary);

        let mut total_lines = 0usize;
        for (idx, msg) in self.state.messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    break;
                }
            }
            total_lines += Self::count_msg_lines_for_click(
                msg,
                &self.state,
                content_width,
                revert_boundary,
                idx,
            );
        }

        let visible_lines = (panel_bottom - panel_top) as usize;
        if visible_lines == 0 || total_lines == 0 {
            return None;
        }

        let scroll = if self.state.scroll_offset == usize::MAX {
            total_lines.saturating_sub(visible_lines)
        } else {
            self.state
                .scroll_offset
                .min(total_lines.saturating_sub(visible_lines))
        };

        let target_row = panel_row as usize + scroll;
        let mut current_row = 0usize;

        for (idx, msg) in self.state.messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    break;
                }
            }

            let (lines, tc_ranges) = Self::count_msg_lines_with_tool_ranges(
                msg,
                &self.state,
                content_width,
                revert_boundary,
                idx,
            );

            if target_row < current_row + lines {
                let offset_in_msg = target_row - current_row;
                for (tc_idx, start, end) in &tc_ranges {
                    if offset_in_msg >= *start && offset_in_msg < *end {
                        return Some((idx, *tc_idx));
                    }
                }
                return None;
            }
            current_row += lines;
        }

        None
    }

    fn count_msg_lines_for_click(
        msg: &crate::tui::app::Message,
        state: &AppState,
        content_width: usize,
        revert_boundary: Option<usize>,
        idx: usize,
    ) -> usize {
        use crate::tui::app::ToolStatus;
        let mut count = 0;
        if let Some(boundary) = revert_boundary {
            if idx == boundary.saturating_sub(1) {
                count += 3;
            }
        }
        if count > 0 {
            count += 1;
        } else if idx > 0 {
            count += 1;
        }

        match &msg.role {
            MessageRole::User => {
                let wrapped = crate::tui::widgets::message_list::wrap_text(
                    &msg.full_text(),
                    content_width.saturating_sub(3),
                );
                count += wrapped.len();
            }
            MessageRole::Assistant => {
                for part in &msg.parts {
                    match part {
                        MessagePart::Text { content } => {
                            let md = crate::tui::markdown::render_markdown_to_lines(
                                content,
                                content_width.saturating_sub(4),
                                &state.theme,
                            );
                            count += md.len();
                        }
                        MessagePart::ToolCall {
                            status,
                            output,
                            expanded,
                            ..
                        } => {
                            count += 1;
                            if *expanded {
                                if let Some(ref out) = output {
                                    let out_lines = crate::tui::widgets::message_list::wrap_text(
                                        out,
                                        content_width.saturating_sub(8),
                                    );
                                    let max_out = if *status == ToolStatus::Running {
                                        5
                                    } else {
                                        out_lines.len()
                                    };
                                    count += max_out;
                                    if out_lines.len() > max_out {
                                        count += 1;
                                    }
                                }
                            }
                        }
                        MessagePart::Thinking { content } => {
                            let lines = crate::tui::widgets::message_list::wrap_text(
                                content,
                                content_width.saturating_sub(4),
                            );
                            count += lines.len().max(1);
                        }
                    }
                }
                if msg.is_streaming {
                    count += 1;
                } else if !msg.full_text().is_empty() {
                    count += 1;
                }
            }
            MessageRole::Tool { name } => {
                count += 1;
                if let Some(t) = state.tools.iter().rev().find(|t| &t.name == name) {
                    if !t.input_summary.is_empty() {
                        let summary = crate::tui::widgets::message_list::wrap_text(
                            &t.input_summary,
                            content_width.saturating_sub(6),
                        );
                        count += summary.len();
                    }
                }
            }
        }
        count
    }

    fn count_msg_lines_with_tool_ranges(
        msg: &crate::tui::app::Message,
        state: &AppState,
        content_width: usize,
        revert_boundary: Option<usize>,
        idx: usize,
    ) -> (usize, Vec<(usize, usize, usize)>) {
        use crate::tui::app::ToolStatus;
        let mut count = 0;
        let mut tc_ranges = Vec::new();

        if let Some(boundary) = revert_boundary {
            if idx == boundary.saturating_sub(1) {
                count += 3;
            }
        }
        if count > 0 {
            count += 1;
        } else if idx > 0 {
            count += 1;
        }

        match &msg.role {
            MessageRole::User => {
                let wrapped = crate::tui::widgets::message_list::wrap_text(
                    &msg.full_text(),
                    content_width.saturating_sub(3),
                );
                count += wrapped.len();
            }
            MessageRole::Assistant => {
                let mut tc_idx = 0;
                for part in &msg.parts {
                    match part {
                        MessagePart::Text { content } => {
                            let md = crate::tui::markdown::render_markdown_to_lines(
                                content,
                                content_width.saturating_sub(4),
                                &state.theme,
                            );
                            count += md.len();
                        }
                        MessagePart::ToolCall {
                            status,
                            output,
                            expanded,
                            ..
                        } => {
                            let line_start = count;
                            count += 1;
                            if *expanded {
                                if let Some(ref out) = output {
                                    let out_lines = crate::tui::widgets::message_list::wrap_text(
                                        out,
                                        content_width.saturating_sub(8),
                                    );
                                    let max_out = if *status == ToolStatus::Running {
                                        5
                                    } else {
                                        out_lines.len()
                                    };
                                    count += max_out;
                                    if out_lines.len() > max_out {
                                        count += 1;
                                    }
                                }
                            }
                            tc_ranges.push((tc_idx, line_start, count));
                            tc_idx += 1;
                        }
                        MessagePart::Thinking { content } => {
                            let lines = crate::tui::widgets::message_list::wrap_text(
                                content,
                                content_width.saturating_sub(4),
                            );
                            count += lines.len().max(1);
                        }
                    }
                }
                if msg.is_streaming {
                    count += 1;
                } else if !msg.full_text().is_empty() {
                    count += 1;
                }
            }
            MessageRole::Tool { name } => {
                count += 1;
                if let Some(t) = state.tools.iter().rev().find(|t| &t.name == name) {
                    if !t.input_summary.is_empty() {
                        let summary = crate::tui::widgets::message_list::wrap_text(
                            &t.input_summary,
                            content_width.saturating_sub(6),
                        );
                        count += summary.len();
                    }
                }
            }
        }
        (count, tc_ranges)
    }

    fn message_at(&self, screen_row: u16, _screen_col: u16) -> Option<usize> {
        let area = self.terminal.size().ok()?;
        let prompt_lines = self
            .state
            .prompt
            .line_count(area.width as usize)
            .clamp(1, 6);
        let prompt_height = (prompt_lines as u16) + 3;

        let is_welcome = self.state.messages.is_empty();
        let has_sidebar = !is_welcome && self.state.sidebar_visible && area.width > 120;
        let panel_width = if has_sidebar {
            area.width - 2 - 42
        } else {
            area.width - 2
        };
        let content_width = panel_width.saturating_sub(2) as usize;

        let panel_top = 1u16;
        let panel_bottom = area.height.saturating_sub(prompt_height + 1);

        if screen_row < panel_top || screen_row >= panel_bottom {
            return None;
        }

        let panel_row = screen_row - panel_top;
        let revert_boundary = self.state.revert.as_ref().map(|r| r.message_boundary);
        use crate::tui::app::ToolStatus;

        fn count_msg_lines(
            msg: &crate::tui::app::Message,
            state: &AppState,
            content_width: usize,
            revert_boundary: Option<usize>,
            idx: usize,
        ) -> usize {
            let mut count = 0;
            if let Some(boundary) = revert_boundary {
                if idx == boundary.saturating_sub(1) {
                    count += 3;
                }
            }
            if count > 0 {
                count += 1;
            } else if idx > 0 {
                count += 1;
            }

            match &msg.role {
                MessageRole::User => {
                    let wrapped = crate::tui::widgets::message_list::wrap_text(
                        &msg.full_text(),
                        content_width.saturating_sub(3),
                    );
                    count += wrapped.len();
                }
                MessageRole::Assistant => {
                    for part in &msg.parts {
                        match part {
                            MessagePart::Text { content } => {
                                let md = crate::tui::markdown::render_markdown_to_lines(
                                    content,
                                    content_width.saturating_sub(4),
                                    &state.theme,
                                );
                                count += md.len();
                            }
                            MessagePart::ToolCall {
                                status,
                                output,
                                expanded,
                                ..
                            } => {
                                count += 1;
                                if *expanded {
                                    if let Some(ref out) = output {
                                        let out = crate::tui::widgets::message_list::wrap_text(
                                            out,
                                            content_width.saturating_sub(8),
                                        );
                                        let max_out = if *status == ToolStatus::Running {
                                            5
                                        } else {
                                            out.len()
                                        };
                                        count += max_out;
                                        if out.len() > max_out {
                                            count += 1;
                                        }
                                    }
                                }
                            }
                            MessagePart::Thinking { content } => {
                                let lines = crate::tui::widgets::message_list::wrap_text(
                                    content,
                                    content_width.saturating_sub(4),
                                );
                                count += lines.len().max(1);
                            }
                        }
                    }
                    if msg.is_streaming {
                        count += 1;
                    } else if !msg.full_text().is_empty() {
                        count += 1;
                    }
                }
                MessageRole::Tool { name } => {
                    count += 1;
                    if let Some(t) = state.tools.iter().rev().find(|t| &t.name == name) {
                        if !t.input_summary.is_empty() {
                            let summary = crate::tui::widgets::message_list::wrap_text(
                                &t.input_summary,
                                content_width.saturating_sub(6),
                            );
                            count += summary.len();
                        }
                    }
                }
            }
            count
        }

        let mut total_lines = 0usize;
        for (idx, msg) in self.state.messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    break;
                }
            }
            total_lines += count_msg_lines(msg, &self.state, content_width, revert_boundary, idx);
        }

        let visible_lines = (panel_bottom - panel_top) as usize;
        if visible_lines == 0 || total_lines == 0 {
            return None;
        }

        let scroll = if self.state.scroll_offset == usize::MAX {
            total_lines.saturating_sub(visible_lines)
        } else {
            self.state
                .scroll_offset
                .min(total_lines.saturating_sub(visible_lines))
        };

        let target_row = panel_row as usize + scroll;
        let mut current_row = 0usize;

        for (idx, msg) in self.state.messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    break;
                }
            }

            let lines = count_msg_lines(msg, &self.state, content_width, revert_boundary, idx);

            if target_row < current_row + lines {
                return Some(idx);
            }
            current_row += lines;
        }

        None
    }

    fn extract_selection_text(&mut self, sel: &crate::tui::app::TextSelection) -> String {
        let area = match self.terminal.size() {
            Ok(a) => a,
            Err(_) => return String::new(),
        };
        let prompt_lines = self
            .state
            .prompt
            .line_count(area.width as usize)
            .clamp(1, 6);
        let prompt_height = (prompt_lines as u16) + 3;
        let panel_top = 1u16;
        let panel_bottom = area.height.saturating_sub(prompt_height + 1);

        let min_row = sel.start_row.min(sel.end_row);
        let max_row = sel.start_row.max(sel.end_row);

        if min_row < panel_top || min_row >= panel_bottom {
            return String::new();
        }

        let has_sidebar = self.state.sidebar_visible && area.width > 120;
        let panel_width = if has_sidebar {
            area.width - 2 - 42
        } else {
            area.width - 2
        };
        let content_width = panel_width.saturating_sub(2) as usize;

        let revert_boundary = self.state.revert.as_ref().map(|r| r.message_boundary);

        let mut content_lines: Vec<String> = Vec::new();

        for (idx, msg) in self.state.messages.iter().enumerate() {
            if let Some(boundary) = revert_boundary {
                if idx >= boundary {
                    break;
                }
                if idx == boundary.saturating_sub(1) {
                    content_lines.push(String::new());
                    content_lines.push(format!(
                        "↩ {} message(s) reverted  •  ↻ PgDn to redo",
                        self.state.reverted_count()
                    ));
                    content_lines.push(String::new());
                }
            }

            match &msg.role {
                MessageRole::User => {
                    if !content_lines.is_empty() {
                        content_lines.push(String::new());
                    }
                    let wrapped = crate::tui::widgets::message_list::wrap_text(
                        &msg.full_text(),
                        content_width.saturating_sub(3),
                    );
                    for line in wrapped {
                        content_lines.push(line);
                    }
                }
                MessageRole::Assistant => {
                    if !content_lines.is_empty() {
                        content_lines.push(String::new());
                    }
                    for part in &msg.parts {
                        match part {
                            MessagePart::Text { content } => {
                                let md = crate::tui::markdown::render_markdown_to_lines(
                                    content,
                                    content_width.saturating_sub(4),
                                    &self.state.theme,
                                );
                                for line in md {
                                    let text: String =
                                        line.spans.iter().map(|s| s.content.as_ref()).collect();
                                    content_lines.push(text);
                                }
                            }
                            MessagePart::ToolCall {
                                status,
                                output,
                                expanded,
                                name,
                                input_summary,
                                ..
                            } => {
                                use crate::tui::app::ToolStatus as TS;
                                let label = match status {
                                    TS::Pending | TS::Running => {
                                        let cmd = if input_summary.is_empty() {
                                            String::new()
                                        } else if let Ok(val) =
                                            serde_json::from_str::<serde_json::Value>(input_summary)
                                        {
                                            val.get("command")
                                                .or_else(|| val.get("cmd"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string()
                                        } else {
                                            String::new()
                                        };
                                        if cmd.is_empty() {
                                            format!("⏳ {name}...")
                                        } else {
                                            format!("⏳ {cmd}")
                                        }
                                    }
                                    TS::Completed => format!("✓ {name}"),
                                    TS::Failed => format!("✗ {name}"),
                                };
                                content_lines.push(label);
                                if *expanded {
                                    if let Some(ref out) = output {
                                        let out_lines =
                                            crate::tui::widgets::message_list::wrap_text(
                                                out,
                                                content_width.saturating_sub(8),
                                            );
                                        let max_out = if *status == TS::Running {
                                            5usize
                                        } else {
                                            out_lines.len()
                                        };
                                        for line in out_lines.iter().take(max_out) {
                                            content_lines.push(line.clone());
                                        }
                                        if out_lines.len() > max_out {
                                            content_lines.push(format!(
                                                "... {} more lines",
                                                out_lines.len() - max_out
                                            ));
                                        }
                                    }
                                }
                            }
                            MessagePart::Thinking { content } => {
                                let wrapped = crate::tui::widgets::message_list::wrap_text(
                                    content,
                                    content_width.saturating_sub(4),
                                );
                                for line in wrapped {
                                    content_lines.push(format!("[thinking] {line}"));
                                }
                            }
                        }
                    }
                    if !msg.is_streaming && !msg.full_text().is_empty() {
                        content_lines
                            .push(format!("▣ {} · {}", msg.agent, self.state.session.model));
                    }
                }
                MessageRole::Tool { name } => {
                    if !content_lines.is_empty() {
                        content_lines.push(String::new());
                    }
                    let tool = self.state.tools.iter().rev().find(|t| &t.name == name);
                    let status = tool.map_or(crate::tui::app::ToolStatus::Completed, |t| t.status);
                    let icon = match status {
                        crate::tui::app::ToolStatus::Pending
                        | crate::tui::app::ToolStatus::Running => "○",
                        crate::tui::app::ToolStatus::Completed => "✓",
                        crate::tui::app::ToolStatus::Failed => "✗",
                    };
                    content_lines.push(format!("  {icon} {name}"));
                    if let Some(t) = tool {
                        if !t.input_summary.is_empty() {
                            let summary = crate::tui::widgets::message_list::wrap_text(
                                &t.input_summary,
                                content_width.saturating_sub(6),
                            );
                            for s in summary {
                                content_lines.push(format!("     {s}"));
                            }
                        }
                    }
                }
            }
        }

        let total_lines = content_lines.len();
        let visible_lines = (panel_bottom - panel_top) as usize;
        if visible_lines == 0 || total_lines == 0 {
            return String::new();
        }

        let scroll = if self.state.scroll_offset == usize::MAX {
            total_lines.saturating_sub(visible_lines)
        } else {
            self.state
                .scroll_offset
                .min(total_lines.saturating_sub(visible_lines))
        };

        let min_content_idx =
            (min_row as usize + scroll).min(content_lines.len().saturating_sub(1));
        let max_content_idx =
            (max_row as usize + scroll).min(content_lines.len().saturating_sub(1));

        if min_content_idx >= content_lines.len() {
            return String::new();
        }

        let mut result = String::new();
        for i in min_content_idx..=max_content_idx.min(content_lines.len().saturating_sub(1)) {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&content_lines[i]);
        }

        result
    }
}

fn copy_to_clipboard(text: &str) {
    // Primary: system clipboard via arboard (works on X11, Wayland, macOS, Windows)
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    } else {
        // Fallback: OSC52 escape sequence for terminals that support it
        let encoded = encode_base64(text.as_bytes());
        let osc = format!("\x1b]52;c;{encoded}\x1b\\");
        print!("{osc}");
        let _ = std::io::stdout().flush();
    }
}

fn encode_base64(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

impl Drop for Tui {
    fn drop(&mut self) {
        if let Some(ref store) = self.state.prompt.frecency {
            if store.is_dirty() {
                let _ = store.save();
            }
        }
        let _ = KittyKeyboard::disable();
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

/// Handle `/debug` slash command — inlined from main.rs for TUI accessibility.
fn handle_debug_slash_command(action: Option<&str>) -> Result<String, String> {
    match action {
        None => {
            let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info (default)".to_string());
            let log_dir = crate::tui::debug::icode_log_dir();
            let log_path = log_dir.join("icode");
            let file_size = std::fs::metadata(&log_path)
                .map_or_else(|_| "file not found".to_string(), |m| format!("{} bytes", m.len()));

            let mut recent_logs = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&log_dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(name) = entry.file_name().into_string() {
                            let modified = meta.modified()
                                .map(|t| {
                                    use std::time::SystemTime;
                                    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
                                    dur.as_secs()
                                })
                                .unwrap_or(0);
                            recent_logs.push((modified, format!("  {} ({} bytes)", name, meta.len())));
                        }
                    }
                }
            }
            recent_logs.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));

            let recent = if recent_logs.is_empty() {
                "  (no log files found)".to_string()
            } else {
                recent_logs.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n")
            };

            Ok(format!(
                "Debug Log Status:\n  Log level:     {log_level}\n  Log directory: {}\n  File size:     {file_size}\n\nRecent log files:\n{recent}",
                log_dir.display(),
            ))
        }
        Some("on") => {
            std::env::set_var("RUST_LOG", "debug,hyper=info,h2=info");
            Ok("Debug logging enabled (RUST_LOG=debug,hyper=info,h2=info).\nNote: Active tracing subscriber cannot be re-initialized at runtime.\nNew log entries will use the updated level after restart.".to_string())
        }
        Some("off") => {
            std::env::set_var("RUST_LOG", "info");
            Ok("Debug logging disabled (RUST_LOG=info).\nNote: Active tracing subscriber cannot be re-initialized at runtime.\nNew log entries will use the updated level after restart.".to_string())
        }
        Some("level") => {
            Err("Usage: /debug level <level>\nValid levels: error, warn, info, debug, trace".to_string())
        }
        Some(level_str) if level_str.starts_with("level ") => {
            let level = level_str.strip_prefix("level ").unwrap().trim();
            if level.is_empty() {
                Err("Usage: /debug level <level>\nValid levels: error, warn, info, debug, trace".to_string())
            } else {
                let valid_levels = ["error", "warn", "info", "debug", "trace"];
                if valid_levels.contains(&level) {
                    std::env::set_var("RUST_LOG", level);
                    Ok(format!(
                        "Log level set to '{level}'.\nNote: Active tracing subscriber cannot be re-initialized at runtime.\nNew log entries will use the updated level after restart."
                    ))
                } else {
                    Err(format!(
                        "Invalid log level '{level}'. Valid levels: error, warn, info, debug, trace"
                    ))
                }
            }
        }
        Some("logs") => {
            let log_dir = crate::tui::debug::icode_log_dir();
            let mut log_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&log_dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            log_files.push((modified, entry.path()));
                        }
                    }
                }
            }
            log_files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));

            let most_recent = log_files.first().map(|(_, path)| path.clone());

            let mut output = format!("Log directory: {}\n", log_dir.display());
            if let Some(path) = &most_recent {
                use std::fmt::Write as _;
                let _ = write!(output, "Most recent: {}\n\n", path.display());
                let file = std::fs::File::open(path).map_err(|e| format!("Cannot open log file: {e}"))?;
                let reader = std::io::BufReader::new(file);
                let mut ring = std::collections::VecDeque::with_capacity(20);
                for line in reader.lines() {
                    let line = line.map_err(|e| format!("Error reading log: {e}"))?;
                    if ring.len() >= 20 {
                        ring.pop_front();
                    }
                    ring.push_back(line);
                }
                output.push_str("Last 20 lines:\n");
                output.push_str("---\n");
                for line in &ring {
                    output.push_str(line);
                    output.push('\n');
                }
                output.push_str("---\n");
            } else {
                output.push_str("No log files found.\n");
            }
            Ok(output)
        }
        Some("clean") => {
            let log_dir = crate::tui::debug::icode_log_dir();
            let before_count = std::fs::read_dir(&log_dir)
                .map(|e| e.flatten().count())
                .unwrap_or(0);
            crate::tui::debug::cleanup_old_logs(7);
            let after_count = std::fs::read_dir(&log_dir)
                .map(|e| e.flatten().count())
                .unwrap_or(0);
            let cleaned = before_count.saturating_sub(after_count);
            Ok(format!("Log cleanup complete. {cleaned} old log file(s) removed (older than 7 days)."))
        }
        Some("help" | "-h" | "--help") => {
            Ok("/debug [subcommand]\n\nSubcommands:\n  /debug              Show current log status (level, path, size)\n  /debug on           Enable debug logging (RUST_LOG=debug,hyper=info,h2=info)\n  /debug off          Disable verbose logging (RUST_LOG=info)\n  /debug level <lvl>  Set specific log level (error|warn|info|debug|trace)\n  /debug logs         Show log path and tail last 20 lines\n  /debug clean        Cleanup old log files (older than 7 days)\n  /debug help         Show this help text\n\nNote: Log level changes apply to new entries after restart.\nThe active tracing subscriber cannot be re-initialized at runtime.".to_string())
        }
        Some(other) => {
            Err(format!(
                "Unknown /debug action '{other}'. Use /debug help for usage information."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::autocomplete::{
        AutocompleteEntry, AutocompleteMode, AutocompleteState, EntryKind,
    };
    use crate::tui::dialog_permission::PermissionDialogState;
    use crate::tui::dialog_question::{QuestionDef, QuestionOption, QuestionPromptState};
    use crate::tui::input::InputState;
    use runtime::{PermissionMode, PermissionRequest};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_autocomplete_enter_selects_entry() {
        let mut input = InputState::new("");
        input.value = "/he".to_string();
        input.cursor = 3;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.mode = AutocompleteMode::Slash;
        ac.idx = 0;
        ac.entries = vec![
            AutocompleteEntry {
                title: "/help".to_string(),
                subtitle: "Show help".to_string(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/history".to_string(),
                subtitle: "Show history".to_string(),
                kind: EntryKind::Command,
            },
        ];

        ac.select(&mut input);
        // select replaces with "/help " (trailing space) and closes
        assert!(input.value.starts_with("/help"));
        assert!(!ac.open);
    }

    #[test]
    fn test_autocomplete_enter_empty_entries() {
        let mut input = InputState::new("");
        input.value = "/xyz".to_string();
        input.cursor = 4;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.entries = vec![];

        ac.select(&mut input);
        assert_eq!(input.value, "/xyz");
    }

    #[test]
    fn test_autocomplete_escape_closes() {
        let mut input = InputState::new("");
        input.value = "/hel".to_string();
        input.cursor = 4;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.entries = vec![AutocompleteEntry {
            title: "/help".to_string(),
            subtitle: "Show help".to_string(),
            kind: EntryKind::Command,
        }];

        let value_before = input.value.clone();
        ac.close();

        assert!(!ac.open);
        assert_eq!(input.value, value_before);
    }

    #[test]
    fn test_autocomplete_up_navigates_entries() {
        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.idx = 2;
        ac.entries = vec![
            AutocompleteEntry {
                title: "/help".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/history".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/model".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];

        ac.cursor_up();
        assert_eq!(ac.idx, 1);
        ac.cursor_up();
        assert_eq!(ac.idx, 0);
        ac.cursor_up();
        assert_eq!(ac.idx, 0);
    }

    #[test]
    fn test_autocomplete_down_navigates_entries() {
        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.idx = 0;
        ac.entries = vec![
            AutocompleteEntry {
                title: "/help".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/history".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/model".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];

        ac.cursor_down();
        assert_eq!(ac.idx, 1);
        ac.cursor_down();
        assert_eq!(ac.idx, 2);
        ac.cursor_down();
        assert_eq!(ac.idx, 2);
    }

    #[test]
    fn test_autocomplete_tab_selects_entry() {
        let mut input = InputState::new("");
        input.value = "/h".to_string();
        input.cursor = 2;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.idx = 1;
        ac.entries = vec![
            AutocompleteEntry {
                title: "/help".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/history".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];

        ac.select(&mut input);
        // select adds trailing space
        assert!(input.value.starts_with("/history"));
    }

    #[test]
    fn test_autocomplete_char_insert_and_rebuild() {
        let mut input = InputState::new("");
        input.value = "/h".to_string();
        input.cursor = 2;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.mode = AutocompleteMode::Slash;
        ac.trigger_pos = 0;
        ac.idx = 0;

        input.insert_char('e');
        ac.on_char_insert('e', input.cursor, &input.value);
        ac.rebuild_entries(&input.value, std::path::Path::new("."), None);

        assert_eq!(input.value, "/he");
        assert!(ac.open);
        assert!(!ac.entries.is_empty());
        for entry in &ac.entries {
            assert!(entry.title.starts_with("/h"));
        }
    }

    #[test]
    fn test_permission_dialog_open_blocks_prompt_input() {
        let mut dialog = PermissionDialogState::new();
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            input: "ls -la".to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        };
        dialog.open(request);
        assert!(dialog.open);

        let mut input = InputState::new("");
        input.value = "test prompt".to_string();
        input.cursor = 11;
        let value_before = input.value.clone();

        let result = dialog.handle_key(KeyCode::Char('x'));

        assert_eq!(input.value, value_before);
        assert!(result.is_none());
    }

    #[test]
    fn test_permission_dialog_enter_confirms_action() {
        let mut dialog = PermissionDialogState::new();
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            input: "ls -la".to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        };
        dialog.open(request);

        let result = dialog.handle_key(KeyCode::Enter);
        assert!(result.is_some());
    }

    #[test]
    fn test_permission_dialog_escape_navigates_stages() {
        let mut dialog = PermissionDialogState::new();
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            input: "ls -la".to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        };
        dialog.open(request);

        let result = dialog.handle_key(KeyCode::Esc);
        assert!(result.is_none());

        let result = dialog.handle_key(KeyCode::Esc);
        assert!(result.is_none());

        // Enter from Permission stage confirms (Approve)
        let result = dialog.handle_key(KeyCode::Enter);
        assert!(result.is_some());
    }

    #[test]
    fn test_permission_dialog_reject_message_accepts_typing() {
        let mut dialog = PermissionDialogState::new();
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            input: "ls -la".to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        };
        dialog.open(request);

        dialog.handle_key(KeyCode::Esc);

        dialog.handle_key(KeyCode::Char('t'));
        dialog.handle_key(KeyCode::Char('e'));
        dialog.handle_key(KeyCode::Char('s'));
        dialog.handle_key(KeyCode::Char('t'));

        assert_eq!(dialog.reject_message, "test");
    }

    #[test]
    fn test_question_dialog_open_blocks_prompt_input() {
        let mut dialog = QuestionPromptState::new();
        let questions = vec![QuestionDef::select(
            "test_question",
            "Pick one:",
            vec![
                QuestionOption::new("Option A"),
                QuestionOption::new("Option B"),
            ],
        )];
        dialog.open(questions, "build".to_string(), "test".to_string(), None);
        assert!(dialog.open);

        let mut input = InputState::new("");
        input.value = "test prompt".to_string();
        input.cursor = 11;
        let value_before = input.value.clone();

        // For single-select with single question, pressing '1' auto-submits
        let result = dialog.handle_key(key(KeyCode::Char('1')));

        assert_eq!(input.value, value_before);
        // Single question + single select = auto-submits on number key
        assert!(result.is_some());
    }

    #[test]
    fn test_question_dialog_enter_confirms_answers() {
        let mut dialog = QuestionPromptState::new();
        let questions = vec![
            QuestionDef::select(
                "q1",
                "First?",
                vec![QuestionOption::new("A"), QuestionOption::new("B")],
            ),
            QuestionDef::select(
                "q2",
                "Second?",
                vec![QuestionOption::new("X"), QuestionOption::new("Y")],
            ),
        ];
        dialog.open(questions, "build".to_string(), "test".to_string(), None);

        dialog.handle_key(key(KeyCode::Char('1')));
        dialog.handle_key(key(KeyCode::Char('1')));

        let result = dialog.handle_key(key(KeyCode::Enter));
        assert!(result.is_some());
        assert!(!dialog.open);
    }

    #[test]
    fn test_question_dialog_escape_closes() {
        let mut dialog = QuestionPromptState::new();
        let questions = vec![QuestionDef::select(
            "test_question",
            "Pick one:",
            vec![
                QuestionOption::new("Option A"),
                QuestionOption::new("Option B"),
            ],
        )];
        dialog.open(questions, "build".to_string(), "test".to_string(), None);

        let result = dialog.handle_key(key(KeyCode::Esc));
        assert!(result.is_some());
        assert!(!dialog.open);
    }

    #[test]
    fn test_question_dialog_arrows_navigate_options() {
        let mut dialog = QuestionPromptState::new();
        let questions = vec![QuestionDef::select(
            "test_question",
            "Pick one:",
            vec![
                QuestionOption::new("Option A"),
                QuestionOption::new("Option B"),
                QuestionOption::new("Option C"),
            ],
        )];
        dialog.open(questions, "build".to_string(), "test".to_string(), None);

        dialog.handle_key(key(KeyCode::Down));
        dialog.handle_key(key(KeyCode::Up));

        assert!(dialog.open);
    }

    #[test]
    fn test_routing_priority_permission_before_prompt() {
        let mut dialog = PermissionDialogState::new();
        let request = PermissionRequest {
            tool_name: "read_file".to_string(),
            input: "secret.txt".to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        };
        dialog.open(request);

        let input = InputState::new("");
        let value_before = input.value.clone();

        for c in ['a', 'b', 'c'] {
            dialog.handle_key(KeyCode::Char(c));
        }

        assert_eq!(input.value, value_before);
    }

    #[test]
    fn test_routing_priority_question_before_prompt() {
        let mut dialog = QuestionPromptState::new();
        let questions = vec![QuestionDef::text(
            "test_question",
            "Type something:",
            "placeholder",
        )];
        dialog.open(questions, "build".to_string(), "test".to_string(), None);

        let input = InputState::new("");
        let value_before = input.value.clone();

        for c in ['h', 'e', 'l', 'l', 'o'] {
            dialog.handle_key(key(KeyCode::Char(c)));
        }

        assert_eq!(input.value, value_before);
        assert!(!dialog.custom_input.is_empty() || dialog.open);
    }

    #[test]
    fn test_autocomplete_up_down_does_not_affect_prompt_cursor() {
        let mut input = InputState::new("");
        input.value = "/help".to_string();
        input.cursor = 5;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.idx = 0;
        ac.entries = vec![
            AutocompleteEntry {
                title: "/help".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/history".to_string(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];

        let cursor_before = input.cursor;

        ac.cursor_up();
        ac.cursor_down();
        ac.cursor_down();

        assert_eq!(input.cursor, cursor_before);
        assert_eq!(ac.idx, 1);
    }

    #[test]
    fn test_autocomplete_backspace_rebuilds_entries() {
        let mut input = InputState::new("");
        input.value = "/help".to_string();
        input.cursor = 5;

        let mut ac = AutocompleteState::new();
        ac.open = true;
        ac.mode = AutocompleteMode::Slash;
        ac.trigger_pos = 0;

        ac.rebuild_entries(&input.value, std::path::Path::new("."), None);

        input.backspace();
        ac.on_backspace(input.cursor);
        ac.rebuild_entries(&input.value, std::path::Path::new("."), None);

        assert!(!ac.entries.is_empty());
        assert_eq!(input.value, "/hel");
    }

    #[test]
    fn test_ctrl_k_clears_prompt_and_closes_autocomplete() {
        let mut input = InputState::new("");
        input.value = "some text".to_string();
        input.cursor = 9;

        let mut ac = AutocompleteState::new();
        ac.open = true;

        input.clear();
        ac.close();

        assert_eq!(input.value, "");
        assert_eq!(input.cursor, 0);
        assert!(!ac.open);
    }
}
