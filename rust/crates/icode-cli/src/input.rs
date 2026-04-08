use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    Submit(String),
    Cancel,
    Exit,
}

/// Completion context detected from the current line and cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionContext {
    /// Completing a slash command name (e.g. "/h" → "/help")
    CommandName,
    /// Completing a file path after "/export " or "/export\t"
    FilePath,
    /// Completing a model name after "/model " or "/model\t"
    ModelName,
    /// Completing a session ID after "/session switch " or "/session switch\t"
    SessionId,
    /// Fallback — no contextual completion available.
    Other,
}

/// Detect what kind of completion is needed based on the line and cursor position.
fn detect_context(line: &str, pos: usize) -> CompletionContext {
    let prefix = &line[..pos];

    if prefix.starts_with("/export ") || prefix.starts_with("/export\t") {
        CompletionContext::FilePath
    } else if prefix.starts_with("/model ") || prefix.starts_with("/model\t") {
        CompletionContext::ModelName
    } else if prefix.starts_with("/session switch ") || prefix.starts_with("/session switch\t") {
        CompletionContext::SessionId
    } else if prefix.starts_with('/') {
        CompletionContext::CommandName
    } else {
        CompletionContext::Other
    }
}

/// Extract the partial path token being completed for file-path context.
fn extract_partial_path(line: &str, pos: usize) -> String {
    let before = &line[..pos];
    // The path argument starts after "/export " (8 chars) or "/export\t" (8 chars)
    if let Some(rest) = before
        .strip_prefix("/export ")
        .or_else(|| before.strip_prefix("/export\t"))
    {
        rest.to_string()
    } else {
        String::new()
    }
}

/// Find the start byte index of the path token for replacement positioning.
fn find_path_start(line: &str, pos: usize) -> usize {
    let before = &line[..pos];
    if let Some(rest) = before
        .strip_prefix("/export ")
        .or_else(|| before.strip_prefix("/export\t"))
    {
        // Path starts 8 bytes into the prefix ("/export " is 8 bytes)
        (pos - rest.len()).min(pos)
    } else {
        pos
    }
}

/// Extract the partial token after a command prefix for model/session completion.
fn extract_after_command(line: &str, pos: usize, command_prefix: &str) -> String {
    let before = &line[..pos];
    before
        .strip_prefix(command_prefix)
        .or_else(|| before.strip_prefix(&command_prefix.replace(' ', "\t")))
        .unwrap_or("")
        .to_string()
}

/// Find the start of the token after the command (for replacement positioning).
fn find_token_start(line: &str, pos: usize, command_prefix: &str) -> usize {
    let before = &line[..pos];
    let prefix_len = if before.starts_with(command_prefix) {
        command_prefix.len()
    } else if before.starts_with(&command_prefix.replace(' ', "\t")) {
        command_prefix.len()
    } else {
        pos
    };
    (pos - before[prefix_len..].len()).min(pos)
}

/// Complete file paths from the filesystem given a partial path.
fn complete_file_path(partial: &str) -> Vec<String> {
    let (dir, file_prefix) = if partial.is_empty() || partial.ends_with('/') {
        (partial.to_string(), String::new())
    } else {
        let path = std::path::Path::new(partial);
        let parent = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (parent, file_name)
    };

    let dir_path = if dir.is_empty() { "." } else { &dir };
    std::fs::read_dir(dir_path)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(std::result::Result::ok))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&file_prefix) {
                let is_dir = entry.file_type().ok()?.is_dir();
                let full = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}/{name}")
                };
                Some(if is_dir { format!("{full}/") } else { full })
            } else {
                None
            }
        })
        .take(20)
        .collect()
}

struct SlashCommandHelper {
    completions: Vec<String>,
    current_line: RefCell<String>,
    available_models: Vec<String>,
    available_sessions: Vec<String>,
}

impl SlashCommandHelper {
    fn new(completions: Vec<String>) -> Self {
        Self {
            completions: normalize_completions(completions),
            current_line: RefCell::new(String::new()),
            available_models: Vec::new(),
            available_sessions: Vec::new(),
        }
    }

    fn reset_current_line(&self) {
        self.current_line.borrow_mut().clear();
    }

    fn current_line(&self) -> String {
        self.current_line.borrow().clone()
    }

    fn set_current_line(&self, line: &str) {
        let mut current = self.current_line.borrow_mut();
        current.clear();
        current.push_str(line);
    }

    fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = normalize_completions(completions);
    }

    fn set_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    fn set_sessions(&mut self, sessions: Vec<String>) {
        self.available_sessions = sessions;
    }
}

impl Completer for SlashCommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let context = detect_context(line, pos);

        let (start, matches) = match context {
            CompletionContext::FilePath => {
                let partial = extract_partial_path(line, pos);
                let start = find_path_start(line, pos);
                let comps = complete_file_path(&partial);
                (start, comps)
            }
            CompletionContext::ModelName => {
                let partial = extract_after_command(line, pos, "/model ");
                let start = find_token_start(line, pos, "/model ");
                let comps: Vec<String> = self
                    .available_models
                    .iter()
                    .filter(|m| m.starts_with(&partial))
                    .take(20).cloned()
                    .collect();
                (start, comps)
            }
            CompletionContext::SessionId => {
                let partial = extract_after_command(line, pos, "/session switch ");
                let start = find_token_start(line, pos, "/session switch ");
                let comps: Vec<String> = self
                    .available_sessions
                    .iter()
                    .filter(|s| s.starts_with(&partial))
                    .take(20).cloned()
                    .collect();
                (start, comps)
            }
            CompletionContext::CommandName => {
                let Some(prefix) = slash_command_prefix(line, pos) else {
                    return Ok((0, Vec::new()));
                };
                let matches: Vec<String> = self
                    .completions
                    .iter()
                    .filter(|candidate| candidate.starts_with(prefix))
                    .cloned()
                    .collect();
                (0, matches)
            }
            CompletionContext::Other => {
                return Ok((pos, Vec::new()));
            }
        };

        let pairs = matches
            .iter()
            .map(|c| Pair {
                display: c.clone(),
                replacement: c.clone(),
            })
            .collect();

        Ok((start, pairs))
    }
}

impl Hinter for SlashCommandHelper {
    type Hint = String;
}

impl Highlighter for SlashCommandHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        self.set_current_line(line);
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, line: &str, _pos: usize, _kind: CmdKind) -> bool {
        self.set_current_line(line);
        false
    }
}

impl Validator for SlashCommandHelper {}
impl Helper for SlashCommandHelper {}

pub struct LineEditor {
    prompt: String,
    editor: Editor<SlashCommandHelper, DefaultHistory>,
}

impl LineEditor {
    #[must_use]
    pub fn new(prompt: impl Into<String>, completions: Vec<String>) -> Self {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .build();
        let mut editor = Editor::<SlashCommandHelper, DefaultHistory>::with_config(config)
            .expect("rustyline editor should initialize");
        editor.set_helper(Some(SlashCommandHelper::new(completions)));
        editor.bind_sequence(KeyEvent(KeyCode::Char('J'), Modifiers::CTRL), Cmd::Newline);
        editor.bind_sequence(KeyEvent(KeyCode::Enter, Modifiers::SHIFT), Cmd::Newline);

        Self {
            prompt: prompt.into(),
            editor,
        }
    }

    pub fn push_history(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.trim().is_empty() {
            return;
        }

        let _ = self.editor.add_history_entry(entry);
    }

    pub fn set_completions(&mut self, completions: Vec<String>) {
        if let Some(helper) = self.editor.helper_mut() {
            helper.set_completions(completions);
        }
    }

    pub fn set_models(&mut self, models: Vec<String>) {
        if let Some(helper) = self.editor.helper_mut() {
            helper.set_models(models);
        }
    }

    pub fn set_sessions(&mut self, sessions: Vec<String>) {
        if let Some(helper) = self.editor.helper_mut() {
            helper.set_sessions(sessions);
        }
    }

    pub fn read_line(&mut self) -> io::Result<ReadOutcome> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return self.read_line_fallback();
        }

        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }

        match self.editor.readline(&self.prompt) {
            Ok(line) => Ok(ReadOutcome::Submit(line)),
            Err(ReadlineError::Interrupted) => {
                let has_input = !self.current_line().is_empty();
                self.finish_interrupted_read()?;
                if has_input {
                    Ok(ReadOutcome::Cancel)
                } else {
                    Ok(ReadOutcome::Exit)
                }
            }
            Err(ReadlineError::Eof) => {
                self.finish_interrupted_read()?;
                Ok(ReadOutcome::Exit)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn current_line(&self) -> String {
        self.editor
            .helper()
            .map_or_else(String::new, SlashCommandHelper::current_line)
    }

    fn finish_interrupted_read(&mut self) -> io::Result<()> {
        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }
        let mut stdout = io::stdout();
        writeln!(stdout)
    }

    fn read_line_fallback(&self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", self.prompt)?;
        stdout.flush()?;

        let mut buffer = String::new();
        let bytes_read = io::stdin().read_line(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(ReadOutcome::Exit);
        }

        while matches!(buffer.chars().last(), Some('\n' | '\r')) {
            buffer.pop();
        }
        Ok(ReadOutcome::Submit(buffer))
    }
}

fn slash_command_prefix(line: &str, pos: usize) -> Option<&str> {
    if pos != line.len() {
        return None;
    }

    let prefix = &line[..pos];
    if !prefix.starts_with('/') {
        return None;
    }

    Some(prefix)
}

fn normalize_completions(completions: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    completions
        .into_iter()
        .filter(|candidate| candidate.starts_with('/'))
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{slash_command_prefix, LineEditor, SlashCommandHelper};
    use rustyline::completion::Completer;
    use rustyline::highlight::Highlighter;
    use rustyline::history::{DefaultHistory, History};
    use rustyline::Context;

    #[test]
    fn extracts_terminal_slash_command_prefixes_with_arguments() {
        assert_eq!(slash_command_prefix("/he", 3), Some("/he"));
        assert_eq!(slash_command_prefix("/help me", 8), Some("/help me"));
        assert_eq!(
            slash_command_prefix("/session switch ses", 19),
            Some("/session switch ses")
        );
        assert_eq!(slash_command_prefix("hello", 5), None);
        assert_eq!(slash_command_prefix("/help", 2), None);
    }

    #[test]
    fn completes_matching_slash_commands() {
        let helper = SlashCommandHelper::new(vec![
            "/help".to_string(),
            "/hello".to_string(),
            "/status".to_string(),
        ]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/he", 3, &ctx)
            .expect("completion should work");

        assert_eq!(start, 0);
        assert_eq!(
            matches
                .into_iter()
                .map(|candidate| candidate.replacement)
                .collect::<Vec<_>>(),
            vec!["/help".to_string(), "/hello".to_string()]
        );
    }

    #[test]
    fn completes_matching_slash_command_arguments() {
        let mut helper = SlashCommandHelper::new(vec![
            "/model".to_string(),
            "/model opus".to_string(),
            "/model sonnet".to_string(),
            "/session switch alpha".to_string(),
        ]);
        helper.set_models(vec!["opus".to_string(), "sonnet".to_string()]);
        helper.set_sessions(vec!["alpha".to_string(), "beta".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/model o", 8, &ctx)
            .expect("completion should work");

        assert_eq!(start, 7);
        let replacements: Vec<_> = matches
            .into_iter()
            .map(|candidate| candidate.replacement)
            .collect();
        assert_eq!(replacements, vec!["opus".to_string()]);
    }

    #[test]
    fn completes_file_paths() {
        let helper = SlashCommandHelper::new(vec!["/export".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_start, matches) = helper
            .complete("/export Cargo", 13, &ctx)
            .expect("completion should work");

        let replacements: Vec<_> = matches.into_iter().map(|c| c.replacement).collect();
        assert!(replacements.iter().any(|r| r.starts_with("Cargo")));
    }

    #[test]
    fn completes_session_ids() {
        let mut helper = SlashCommandHelper::new(vec!["/session switch".to_string()]);
        helper.set_sessions(vec!["alpha".to_string(), "beta".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/session switch a", 17, &ctx)
            .expect("completion should work");

        assert_eq!(start, 16);
        let replacements: Vec<_> = matches.into_iter().map(|c| c.replacement).collect();
        assert_eq!(replacements, vec!["alpha".to_string()]);
    }

    #[test]
    fn ignores_non_slash_command_completion_requests() {
        let helper = SlashCommandHelper::new(vec!["/help".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_, matches) = helper
            .complete("hello", 5, &ctx)
            .expect("completion should work");

        assert!(matches.is_empty());
    }

    #[test]
    fn tracks_current_buffer_through_highlighter() {
        let helper = SlashCommandHelper::new(Vec::new());
        let _ = helper.highlight("draft", 5);

        assert_eq!(helper.current_line(), "draft");
    }

    #[test]
    fn push_history_ignores_blank_entries() {
        let mut editor = LineEditor::new("> ", vec!["/help".to_string()]);
        editor.push_history("   ");
        editor.push_history("/help");

        assert_eq!(editor.editor.history().len(), 1);
    }

    #[test]
    fn set_completions_replaces_and_normalizes_candidates() {
        let mut editor = LineEditor::new("> ", vec!["/help".to_string()]);
        editor.set_completions(vec![
            "/model opus".to_string(),
            "/model opus".to_string(),
            "status".to_string(),
        ]);

        let helper = editor.editor.helper().expect("helper should exist");
        assert_eq!(helper.completions, vec!["/model opus".to_string()]);
    }
}
