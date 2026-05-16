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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent as CTKeyEvent, KeyEventKind, KeyModifiers,
    };

    #[test]
    fn test_parsed_key_ctrl_modifier() {
        let key = CTKeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let parsed: ParsedKey = key.into();
        assert!(parsed.ctrl());
        assert!(!parsed.meta());
        assert!(!parsed.shift());
    }

    #[test]
    fn test_parsed_key_meta_modifier() {
        let key = CTKeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        let parsed: ParsedKey = key.into();
        assert!(!parsed.ctrl());
        assert!(parsed.meta());
        assert!(!parsed.shift());
    }

    #[test]
    fn test_parsed_key_shift_modifier() {
        let key = CTKeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT);
        let parsed: ParsedKey = key.into();
        assert!(!parsed.ctrl());
        assert!(!parsed.meta());
        assert!(parsed.shift());
    }

    #[test]
    fn test_parsed_key_no_modifiers() {
        let key = CTKeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let parsed: ParsedKey = key.into();
        assert!(!parsed.ctrl());
        assert!(!parsed.meta());
        assert!(!parsed.shift());
        assert_eq!(parsed.code, KeyCode::Enter);
    }

    #[test]
    fn test_parsed_key_roundtrip() {
        let original = CTKeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let parsed: ParsedKey = original.into();
        let roundtrip: KeyEvent = KeyEvent::from(&parsed);
        assert_eq!(roundtrip.code, KeyCode::Char('a'));
        assert_eq!(roundtrip.modifiers, KeyModifiers::CONTROL);
        assert_eq!(roundtrip.kind, KeyEventKind::Press);
    }

    #[test]
    fn test_event_key_variant() {
        let key = CTKeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let event = Event::Key(key);
        match event {
            Event::Key(k) => assert_eq!(k.code, KeyCode::Esc),
            _ => panic!("Expected Event::Key"),
        }
    }

    #[test]
    fn test_event_resize_variant() {
        let event = Event::Resize(120, 40);
        match event {
            Event::Resize(w, h) => {
                assert_eq!(w, 120);
                assert_eq!(h, 40);
            }
            _ => panic!("Expected Event::Resize"),
        }
    }

    #[test]
    fn test_event_tick_variant() {
        let event = Event::Tick;
        assert!(matches!(event, Event::Tick));
    }

    #[test]
    fn test_event_loop_new() {
        let loop_ = EventLoop::new(250);
        let _ = loop_;
    }
}
