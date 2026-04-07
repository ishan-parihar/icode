use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

/// A single key combination (key code + modifiers).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombination {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
}

/// All configurable key actions in the TUI.
///
/// Each variant represents a semantically meaningful user action.
/// Only actions that currently exist as hardcoded match arms are included.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyAction {
    // Top-level actions
    Quit,
    ModelPicker,
    CommandPalette,
    LeaderKey,
    Help,
    ScrollPageUp,
    ScrollPageDown,
    ToggleSidebar,
    ToggleDetails,

    // Leader key sub-actions
    LeaderUndo,
    LeaderRedo,
    LeaderModelPicker,
    LeaderNewSession,
    LeaderSessions,
    LeaderToggleSidebar,
    LeaderCommandPalette,

    // Dialog navigation (shared across dialogs)
    DialogUp,
    DialogDown,
    DialogConfirm,
    DialogCancel,
    DialogSearch,
    DialogPageUp,
    DialogPageDown,

    // Model picker specific
    ModelPickerFavorite,

    // Message action dialog
    MessageActionRevert,
    MessageActionCopy,
    MessageActionFork,
}

/// Registry of keybinds with defaults and config overrides.
pub struct KeybindRegistry {
    defaults: HashMap<KeyAction, Vec<KeyCombination>>,
    overrides: HashMap<KeyAction, Vec<KeyCombination>>,
}

impl KeybindRegistry {
    pub fn new() -> Self {
        Self {
            defaults: HashMap::new(),
            overrides: HashMap::new(),
        }
    }

    /// Populate with all current hardcoded keybinds from runner.rs.
    pub fn populate_defaults(&mut self) {
        macro_rules! key {
            ($code:expr) => {
                KeyCombination {
                    key: $code,
                    modifiers: KeyModifiers::empty(),
                }
            };
            (ctrl $code:expr) => {
                KeyCombination {
                    key: $code,
                    modifiers: KeyModifiers::CONTROL,
                }
            };
            (alt $code:expr) => {
                KeyCombination {
                    key: $code,
                    modifiers: KeyModifiers::ALT,
                }
            };
            (shift $code:expr) => {
                KeyCombination {
                    key: $code,
                    modifiers: KeyModifiers::SHIFT,
                }
            };
        }

        macro_rules! bind {
            ($self:expr, $action:expr, $($combo:expr),+ $(,)?) => {
                $self.defaults.insert($action, vec![$($combo),+]);
            };
        }

        // Top-level
        bind!(self, KeyAction::Quit, key!(ctrl KeyCode::Char('c')));
        bind!(self, KeyAction::ModelPicker, key!(ctrl KeyCode::Char('m')));
        bind!(
            self,
            KeyAction::CommandPalette,
            key!(ctrl KeyCode::Char('p'))
        );
        bind!(self, KeyAction::LeaderKey, key!(ctrl KeyCode::Char('x')));
        bind!(self, KeyAction::Help, key!(KeyCode::Char('?')));
        bind!(self, KeyAction::ScrollPageUp, key!(KeyCode::PageUp));
        bind!(self, KeyAction::ScrollPageDown, key!(KeyCode::PageDown));
        bind!(self, KeyAction::ToggleSidebar, key!(alt KeyCode::Char('s')));
        bind!(
            self,
            KeyAction::ToggleDetails,
            key!(ctrl KeyCode::Char('t'))
        );

        // Leader sub-actions
        bind!(self, KeyAction::LeaderUndo, key!(KeyCode::Char('u')));
        bind!(self, KeyAction::LeaderRedo, key!(KeyCode::Char('r')));
        bind!(self, KeyAction::LeaderModelPicker, key!(KeyCode::Char('m')));
        bind!(self, KeyAction::LeaderNewSession, key!(KeyCode::Char('n')));
        bind!(self, KeyAction::LeaderSessions, key!(KeyCode::Char('l')));
        bind!(
            self,
            KeyAction::LeaderToggleSidebar,
            key!(KeyCode::Char('b'))
        );
        bind!(
            self,
            KeyAction::LeaderCommandPalette,
            key!(KeyCode::Char('a'))
        );

        // Dialog navigation
        bind!(self, KeyAction::DialogUp, key!(KeyCode::Up));
        bind!(self, KeyAction::DialogDown, key!(KeyCode::Down));
        bind!(self, KeyAction::DialogConfirm, key!(KeyCode::Enter));
        bind!(self, KeyAction::DialogCancel, key!(KeyCode::Esc));
        bind!(self, KeyAction::DialogSearch, key!(KeyCode::Char('/')));
        bind!(self, KeyAction::DialogPageUp, key!(KeyCode::PageUp));
        bind!(self, KeyAction::DialogPageDown, key!(KeyCode::PageDown));

        // Model picker
        bind!(
            self,
            KeyAction::ModelPickerFavorite,
            key!(ctrl KeyCode::Char('f'))
        );

        // Message action dialog (uses only key codes, not full KeyEvent)
        bind!(
            self,
            KeyAction::MessageActionRevert,
            key!(KeyCode::Char('r'))
        );
        bind!(self, KeyAction::MessageActionCopy, key!(KeyCode::Char('c')));
        bind!(self, KeyAction::MessageActionFork, key!(KeyCode::Char('f')));
    }

    /// Apply overrides from config key-value pairs.
    ///
    /// Format: action -> key combo string, e.g.
    ///   "command_palette" -> "ctrl+p"
    ///   "toggle_sidebar" -> "alt+s"
    ///   "help" -> "?"
    ///   "leader_undo" -> "u"
    pub fn apply_overrides(&mut self, overrides: &HashMap<String, String>) {
        for (action_str, combo_str) in overrides {
            if let Some(action) = Self::parse_action(action_str) {
                if let Some(combo) = Self::parse_combo(combo_str) {
                    self.overrides.insert(action, vec![combo]);
                }
            }
        }
    }

    /// Parse a string like "command_palette" into a KeyAction.
    fn parse_action(s: &str) -> Option<KeyAction> {
        match s.to_lowercase().as_str() {
            "quit" => Some(KeyAction::Quit),
            "model_picker" | "modelpicker" => Some(KeyAction::ModelPicker),
            "command_palette" | "commandpalette" => Some(KeyAction::CommandPalette),
            "leader" | "leader_key" | "leaderkey" => Some(KeyAction::LeaderKey),
            "help" => Some(KeyAction::Help),
            "scroll_page_up" | "scrollpageup" => Some(KeyAction::ScrollPageUp),
            "scroll_page_down" | "scrollpagedown" => Some(KeyAction::ScrollPageDown),
            "toggle_sidebar" | "togglesidebar" => Some(KeyAction::ToggleSidebar),
            "toggle_details" | "toggledetails" => Some(KeyAction::ToggleDetails),
            "leader_undo" | "leaderundo" => Some(KeyAction::LeaderUndo),
            "leader_redo" | "leaderredo" => Some(KeyAction::LeaderRedo),
            "leader_model" | "leadermodel" | "leader_model_picker" => {
                Some(KeyAction::LeaderModelPicker)
            }
            "leader_new_session" | "leadernewsession" => Some(KeyAction::LeaderNewSession),
            "leader_sessions" | "leadersessions" => Some(KeyAction::LeaderSessions),
            "leader_toggle_sidebar" | "leadertogglesidebar" => Some(KeyAction::LeaderToggleSidebar),
            "leader_command_palette" | "leadercommandpalette" => {
                Some(KeyAction::LeaderCommandPalette)
            }
            "dialog_up" | "dialogup" => Some(KeyAction::DialogUp),
            "dialog_down" | "dialogdown" => Some(KeyAction::DialogDown),
            "dialog_confirm" | "dialogconfirm" => Some(KeyAction::DialogConfirm),
            "dialog_cancel" | "dialogcancel" => Some(KeyAction::DialogCancel),
            "dialog_search" | "dialogsearch" => Some(KeyAction::DialogSearch),
            "dialog_page_up" | "dialogpageup" => Some(KeyAction::DialogPageUp),
            "dialog_page_down" | "dialogpagedown" => Some(KeyAction::DialogPageDown),
            "model_picker_favorite" | "modelpickerfavorite" => Some(KeyAction::ModelPickerFavorite),
            "message_action_revert" | "messageactionrevert" => Some(KeyAction::MessageActionRevert),
            "message_action_copy" | "messageactioncopy" => Some(KeyAction::MessageActionCopy),
            "message_action_fork" | "messageactionfork" => Some(KeyAction::MessageActionFork),
            _ => None,
        }
    }

    /// Parse a key combo string like "ctrl+p" into a KeyCombination.
    fn parse_combo(s: &str) -> Option<KeyCombination> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let parts: Vec<&str> = s.split('+').collect();
        let mut modifiers = KeyModifiers::empty();
        let key_part = parts.last()?.trim();

        for part in parts.iter().take(parts.len() - 1) {
            match part.trim().to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" | "option" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                "super" | "cmd" | "win" => modifiers |= KeyModifiers::SUPER,
                _ => {}
            }
        }

        let key = Self::parse_key_code(key_part)?;
        Some(KeyCombination { key, modifiers })
    }

    /// Parse a key code string like "p", "enter", "up", "f1", etc.
    fn parse_key_code(s: &str) -> Option<KeyCode> {
        let s = s.trim();

        // Single character
        if s.len() == 1 {
            return Some(KeyCode::Char(s.chars().next().unwrap_or(' ')));
        }

        match s.to_lowercase().as_str() {
            "enter" | "return" => Some(KeyCode::Enter),
            "esc" | "escape" => Some(KeyCode::Esc),
            "tab" => Some(KeyCode::Tab),
            "backtab" | "shift_tab" | "shift+tab" => Some(KeyCode::BackTab),
            "backspace" => Some(KeyCode::Backspace),
            "delete" | "del" => Some(KeyCode::Delete),
            "insert" | "ins" => Some(KeyCode::Insert),
            "up" | "uparrow" | "arrow_up" => Some(KeyCode::Up),
            "down" | "downarrow" | "arrow_down" => Some(KeyCode::Down),
            "left" | "leftarrow" | "arrow_left" => Some(KeyCode::Left),
            "right" | "rightarrow" | "arrow_right" => Some(KeyCode::Right),
            "home" => Some(KeyCode::Home),
            "end" => Some(KeyCode::End),
            "pageup" | "page_up" | "pgup" => Some(KeyCode::PageUp),
            "pagedown" | "page_down" | "pgdn" => Some(KeyCode::PageDown),
            "space" => Some(KeyCode::Char(' ')),
            "f1" => Some(KeyCode::F(1)),
            "f2" => Some(KeyCode::F(2)),
            "f3" => Some(KeyCode::F(3)),
            "f4" => Some(KeyCode::F(4)),
            "f5" => Some(KeyCode::F(5)),
            "f6" => Some(KeyCode::F(6)),
            "f7" => Some(KeyCode::F(7)),
            "f8" => Some(KeyCode::F(8)),
            "f9" => Some(KeyCode::F(9)),
            "f10" => Some(KeyCode::F(10)),
            "f11" => Some(KeyCode::F(11)),
            "f12" => Some(KeyCode::F(12)),
            _ => {
                // Try as single char fallback
                if s.len() == 1 {
                    Some(KeyCode::Char(s.chars().next().unwrap()))
                } else {
                    None
                }
            }
        }
    }

    /// Check if the given KeyEvent matches the specified action.
    ///
    /// Checks overrides first, then falls back to defaults.
    pub fn matches(&self, action: &KeyAction, key: &KeyEvent) -> bool {
        let combos = self
            .overrides
            .get(action)
            .or_else(|| self.defaults.get(action));

        if let Some(combos) = combos {
            combos.iter().any(|combo| {
                // For BackTab, we match on code only (Shift is implicit)
                if combo.key == KeyCode::BackTab {
                    return key.code == KeyCode::BackTab;
                }
                key.code == combo.key && key.modifiers == combo.modifiers
            })
        } else {
            false
        }
    }

    /// Check if the given KeyCode matches the specified action (no modifiers).
    ///
    /// Used for message action dialog which only passes KeyCode.
    pub fn matches_key_code(&self, action: &KeyAction, code: KeyCode) -> bool {
        let combos = self
            .overrides
            .get(action)
            .or_else(|| self.defaults.get(action));

        if let Some(combos) = combos {
            combos.iter().any(|combo| combo.key == code)
        } else {
            false
        }
    }

    /// Return a human-readable string for the primary binding of an action.
    ///
    /// e.g. "Ctrl+P", "Alt+S", "?", "u"
    pub fn print(&self, action: &KeyAction) -> String {
        let combos = self
            .overrides
            .get(action)
            .or_else(|| self.defaults.get(action));

        match combos.and_then(|v| v.first()) {
            Some(combo) => Self::format_combo(combo),
            None => "unbound".to_string(),
        }
    }

    fn format_combo(combo: &KeyCombination) -> String {
        let mut parts = Vec::new();

        if combo.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl".to_string());
        }
        if combo.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt".to_string());
        }
        if combo.modifiers.contains(KeyModifiers::SHIFT) {
            // Shift is implicit for uppercase chars, only show for special keys
            if !matches!(combo.key, KeyCode::Char(c) if c.is_ascii_uppercase()) {
                parts.push("Shift".to_string());
            }
        }
        if combo.modifiers.contains(KeyModifiers::SUPER) {
            parts.push("Super".to_string());
        }

        let key_str = match combo.key {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "Tab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            _ => format!("{:?}", combo.key),
        };

        parts.push(key_str);
        parts.join("+")
    }
}

impl Default for KeybindRegistry {
    fn default() -> Self {
        let mut r = Self::new();
        r.populate_defaults();
        r
    }
}
