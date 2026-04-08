use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind, MouseEvent};
use std::time::Duration;

/// Structured key event with convenient accessor methods.
/// With Kitty keyboard enabled, modifiers are properly reported for all keys.
#[derive(Debug, Clone)]
pub struct ParsedKey {
    pub code: event::KeyCode,
    pub modifiers: event::KeyModifiers,
}

impl ParsedKey {
    pub const fn ctrl(&self) -> bool {
        self.modifiers.contains(event::KeyModifiers::CONTROL)
    }

    pub const fn meta(&self) -> bool {
        self.modifiers.contains(event::KeyModifiers::ALT)
    }

    pub const fn shift(&self) -> bool {
        self.modifiers.contains(event::KeyModifiers::SHIFT)
    }
}

impl From<KeyEvent> for ParsedKey {
    fn from(key: KeyEvent) -> Self {
        Self {
            code: key.code,
            modifiers: key.modifiers,
        }
    }
}

impl From<&ParsedKey> for KeyEvent {
    fn from(key: &ParsedKey) -> Self {
        KeyEvent {
            code: key.code,
            modifiers: key.modifiers,
            kind: KeyEventKind::Press,
            state: event::KeyEventState::empty(),
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
}

pub struct EventLoop {
    tick_rate: Duration,
}

impl EventLoop {
    pub const fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    pub fn next(&self) -> std::io::Result<Event> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Ok(Event::Key(key)),
                CrosstermEvent::Mouse(mouse) => Ok(Event::Mouse(mouse)),
                CrosstermEvent::Resize(w, h) => Ok(Event::Resize(w, h)),
                _ => Ok(Event::Tick),
            }
        } else {
            Ok(Event::Tick)
        }
    }
}
