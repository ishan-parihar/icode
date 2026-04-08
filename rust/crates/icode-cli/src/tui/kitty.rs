//! Kitty Keyboard Protocol support.
//!
//! Enables disambiguated key events via crossterm's native
//! `PushKeyboardEnhancementFlags` / `PopKeyboardEnhancementFlags` API,
//! allowing the TUI to distinguish Escape from Alt+key and recognize
//! Shift/Ctrl modifiers on special keys.

use crossterm::event::{PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use crossterm::execute;
use std::io::{self, Write};

/// Sends CSI escape sequences to enable disambiguated key reporting.
/// Pushes flags: `DISAMBIGUATE_ESCAPE_CODES` | `REPORT_EVENT_TYPES` | `REPORT_ALTERNATE_KEYS`.
/// Reset via `PopKeyboardEnhancementFlags` on exit.
#[derive(Debug)]
pub struct KittyKeyboard;

impl KittyKeyboard {
    /// Enable Kitty keyboard protocol. Sends CSI ? 1/2/3 u sequences.
    pub fn enable() -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )?;
        stdout.flush()?;
        Ok(())
    }

    /// Disable Kitty keyboard protocol. Sends CSI > 0 u reset sequence.
    pub fn disable() -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, PopKeyboardEnhancementFlags)?;
        stdout.flush()?;
        Ok(())
    }
}
