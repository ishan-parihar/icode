use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use std::collections::HashMap;

use crate::tui::theme::Theme;

/// Named extension points (slots) in the TUI that plugins can register content for.
///
/// Each slot represents a specific location in the UI where plugin-rendered
/// content can be injected. Slots are resolved by priority, with lower numbers
/// rendered first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotId {
    /// Logo/branding area on the home screen.
    HomeLogo,
    /// Main prompt area on the home screen.
    HomePrompt,
    /// Right side of the home prompt area.
    HomePromptRight,
    /// Bottom area of the home screen.
    HomeBottom,
    /// Footer line on the home screen.
    HomeFooter,
    /// Main prompt input area during a session.
    SessionPrompt,
    /// Right side of the session prompt area.
    SessionPromptRight,
    /// Title area of the sidebar.
    SidebarTitle,
    /// Content area of the sidebar.
    SidebarContent,
    /// Footer area of the sidebar.
    SidebarFooter,
    /// Overlay rendered on top of the entire app.
    AppOverlay,
}

impl SlotId {
    /// All known slot IDs, useful for iteration and rendering.
    pub const ALL: &'static [SlotId] = &[
        SlotId::HomeLogo,
        SlotId::HomePrompt,
        SlotId::HomePromptRight,
        SlotId::HomeBottom,
        SlotId::HomeFooter,
        SlotId::SessionPrompt,
        SlotId::SessionPromptRight,
        SlotId::SidebarTitle,
        SlotId::SidebarContent,
        SlotId::SidebarFooter,
        SlotId::AppOverlay,
    ];
}

/// Content registered by a plugin for a specific UI slot.
#[derive(Debug, Clone)]
pub struct SlotContent {
    /// The plugin that registered this content.
    pub plugin_name: String,
    /// Text lines to render.
    pub lines: Vec<Line<'static>>,
    /// Priority for ordering within the slot (lower = first).
    pub priority: u8,
}

/// Registry for plugin slot content.
///
/// Manages the mapping from `SlotId` to plugin-provided content, supporting
/// priority-based ordering and plugin-scoped cleanup.
pub struct SlotRegistry {
    slots: HashMap<SlotId, Vec<SlotContent>>,
}

impl SlotRegistry {
    /// Create a new empty slot registry.
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    /// Register content for a specific slot.
    ///
    /// If the same plugin already has content for this slot, the old content
    /// is replaced.
    pub fn register(&mut self, slot: SlotId, content: SlotContent) {
        let entries = self.slots.entry(slot).or_default();
        entries.retain(|e| e.plugin_name != content.plugin_name);
        entries.push(content);
        entries.sort_by_key(|e| e.priority);
    }

    /// Get all content for a slot, sorted by priority (lowest first).
    pub fn get(&self, slot: &SlotId) -> Vec<&SlotContent> {
        match self.slots.get(slot) {
            Some(entries) => entries.iter().collect(),
            None => Vec::new(),
        }
    }

    /// Remove all content registered by a specific plugin.
    pub fn clear(&mut self, plugin_name: &str) {
        for entries in self.slots.values_mut() {
            entries.retain(|e| e.plugin_name != plugin_name);
        }
        self.slots.retain(|_, v| !v.is_empty());
    }

    /// Remove content for a specific slot from a specific plugin.
    pub fn remove_plugin_slot(&mut self, plugin_name: &str, slot: SlotId) {
        if let Some(entries) = self.slots.get_mut(&slot) {
            entries.retain(|e| e.plugin_name != plugin_name);
            if entries.is_empty() {
                self.slots.remove(&slot);
            }
        }
    }

    /// Render all content for a slot within the given area.
    ///
    /// Content is rendered vertically, one slot entry after another.
    /// The theme controls the text color for each line.
    pub fn render_slot(&self, frame: &mut Frame, slot_id: SlotId, area: Rect, theme: &Theme) {
        let contents = self.get(&slot_id);
        if contents.is_empty() {
            return;
        }

        let mut all_lines: Vec<Line<'static>> = Vec::new();
        for (i, content) in contents.iter().enumerate() {
            if i > 0 {
                all_lines.push(Line::from(""));
            }
            for line in &content.lines {
                let styled_line = Line::from(
                    line.spans
                        .iter()
                        .map(|span| {
                            if span.style.fg.is_none() {
                                Span::styled(
                                    span.content.clone(),
                                    Style::default()
                                        .fg(theme.text)
                                        .add_modifier(Modifier::empty()),
                                )
                            } else {
                                span.clone()
                            }
                        })
                        .collect::<Vec<_>>(),
                );
                all_lines.push(styled_line);
            }
        }

        let widget = ratatui::widgets::Paragraph::new(all_lines);
        frame.render_widget(widget, area);
    }

    /// Check if a slot has any content.
    pub fn has_content(&self, slot: &SlotId) -> bool {
        self.slots.get(slot).is_some_and(|v| !v.is_empty())
    }

    /// Get the total number of registered slot entries across all slots.
    pub fn total_entries(&self) -> usize {
        self.slots.values().map(Vec::len).sum()
    }

    /// Get all slot IDs that have content.
    pub fn active_slots(&self) -> Vec<SlotId> {
        self.slots.keys().copied().collect()
    }
}

impl Default for SlotRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_content(plugin: &str, text: &str, priority: u8) -> SlotContent {
        SlotContent {
            plugin_name: plugin.to_string(),
            lines: vec![Line::from(text.to_string())],
            priority,
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = SlotRegistry::new();
        assert_eq!(registry.total_entries(), 0);
        assert!(registry.get(&SlotId::HomeLogo).is_empty());
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("test-plugin", "Hello", 1));

        let contents = registry.get(&SlotId::HomeLogo);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].plugin_name, "test-plugin");
    }

    #[test]
    fn test_priority_sorting() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("plugin-b", "Second", 10));
        registry.register(SlotId::HomeLogo, make_content("plugin-a", "First", 1));
        registry.register(SlotId::HomeLogo, make_content("plugin-c", "Third", 20));

        let contents = registry.get(&SlotId::HomeLogo);
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0].plugin_name, "plugin-a");
        assert_eq!(contents[1].plugin_name, "plugin-b");
        assert_eq!(contents[2].plugin_name, "plugin-c");
    }

    #[test]
    fn test_replace_same_plugin() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("my-plugin", "Old", 1));
        registry.register(SlotId::HomeLogo, make_content("my-plugin", "New", 2));

        let contents = registry.get(&SlotId::HomeLogo);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].lines[0].to_string(), "New");
    }

    #[test]
    fn test_clear_by_plugin() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("plugin-a", "A1", 1));
        registry.register(SlotId::HomeLogo, make_content("plugin-b", "B1", 2));
        registry.register(SlotId::SidebarFooter, make_content("plugin-a", "A2", 1));

        registry.clear("plugin-a");

        let home = registry.get(&SlotId::HomeLogo);
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].plugin_name, "plugin-b");

        let sidebar = registry.get(&SlotId::SidebarFooter);
        assert!(sidebar.is_empty());
    }

    #[test]
    fn test_remove_plugin_slot() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("plugin-a", "A1", 1));
        registry.register(SlotId::HomeLogo, make_content("plugin-b", "B1", 2));

        registry.remove_plugin_slot("plugin-a", SlotId::HomeLogo);

        let contents = registry.get(&SlotId::HomeLogo);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].plugin_name, "plugin-b");
    }

    #[test]
    fn test_has_content() {
        let mut registry = SlotRegistry::new();
        assert!(!registry.has_content(&SlotId::HomeLogo));

        registry.register(SlotId::HomeLogo, make_content("test", "Hello", 1));
        assert!(registry.has_content(&SlotId::HomeLogo));
        assert!(!registry.has_content(&SlotId::SidebarFooter));
    }

    #[test]
    fn test_active_slots() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("p1", "A", 1));
        registry.register(SlotId::SidebarFooter, make_content("p2", "B", 1));

        let active = registry.active_slots();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&SlotId::HomeLogo));
        assert!(active.contains(&SlotId::SidebarFooter));
    }

    #[test]
    fn test_total_entries() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("p1", "A", 1));
        registry.register(SlotId::HomeLogo, make_content("p2", "B", 2));
        registry.register(SlotId::SidebarFooter, make_content("p1", "C", 1));

        assert_eq!(registry.total_entries(), 3);
    }

    #[test]
    fn test_clear_empty_slots_removed() {
        let mut registry = SlotRegistry::new();
        registry.register(SlotId::HomeLogo, make_content("p1", "A", 1));
        registry.clear("p1");

        assert_eq!(registry.total_entries(), 0);
        assert!(registry.active_slots().is_empty());
    }

    #[test]
    fn test_default_impl() {
        let registry = SlotRegistry::default();
        assert_eq!(registry.total_entries(), 0);
    }

    #[test]
    fn test_multiple_lines_per_content() {
        let mut registry = SlotRegistry::new();
        let content = SlotContent {
            plugin_name: "multi".to_string(),
            lines: vec![
                Line::from("Line 1"),
                Line::from("Line 2"),
                Line::from("Line 3"),
            ],
            priority: 1,
        };
        registry.register(SlotId::HomeBottom, content);

        let contents = registry.get(&SlotId::HomeBottom);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].lines.len(), 3);
    }

    #[test]
    fn test_slot_id_all_count() {
        assert_eq!(SlotId::ALL.len(), 11);
    }
}
