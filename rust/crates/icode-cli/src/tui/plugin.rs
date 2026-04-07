use crate::tui::app::AppState;
use crate::tui::kv::KvStore;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;
use std::collections::HashMap;

/// Named UI injection points where plugins can render content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginSlot {
    /// Left side of the status bar (footer).
    StatusBarLeft,
    /// Right side of the status bar (footer).
    StatusBarRight,
    /// Bottom of the sidebar panel.
    SidebarBottom,
    /// Footer line appended after a message in the message list.
    MessageFooter,
}

/// Visual style for slot content rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStyle {
    /// Default text color.
    Default,
    /// Accent color (purple in dark theme).
    Accent,
    /// Success color (green).
    Success,
    /// Warning color (orange/yellow).
    Warning,
}

impl SlotStyle {
    /// Resolve the slot style to an actual color from the theme.
    pub fn color(self, theme: &Theme) -> Color {
        match self {
            SlotStyle::Default => theme.text,
            SlotStyle::Accent => theme.accent,
            SlotStyle::Success => theme.success,
            SlotStyle::Warning => theme.warning,
        }
    }
}

/// Content registered by a plugin for a specific UI slot.
#[derive(Debug, Clone)]
pub struct SlotContent {
    /// The plugin that registered this content.
    pub plugin_id: String,
    /// Text lines to render.
    pub lines: Vec<String>,
    /// Visual style for rendering.
    pub style: SlotStyle,
}

/// A full-screen route (view) that a plugin can register.
///
/// Routes appear in the command palette and, when selected, replace the main
/// content area with a custom-rendered view.
pub struct PluginRoute {
    /// Unique identifier, e.g. "sidebar-info:dashboard".
    pub id: String,
    /// Display name shown in the command palette.
    pub title: String,
    /// Icon character or emoji, e.g. "📊".
    pub icon: String,
    /// Category for grouping in the command palette.
    pub category: String,
    /// Render function: receives the frame, content area, app state, and theme.
    pub render_fn: Box<dyn Fn(&mut Frame, Rect, &AppState, Theme) + Send + Sync>,
}

/// Mutable API surface exposed to plugins during command execution.
///
/// Plugins receive a `&mut PluginApi` when their command callback is invoked,
/// giving them safe, scoped access to application state, the KV store, and theme.
pub struct PluginApi<'a> {
    /// Mutable access to the current application state.
    pub state: &'a mut AppState,
    /// Mutable access to the persistent key-value store.
    pub kv: &'a mut KvStore,
    /// Immutable access to the current theme palette.
    pub theme: &'a Theme,
}

impl PluginApi<'_> {
    /// Show a toast notification to the user.
    pub fn toast(&mut self, message: impl Into<String>) {
        self.state.add_toast(message, crate::tui::ToastKind::Info);
    }

    /// Show a success toast notification.
    pub fn toast_success(&mut self, message: impl Into<String>) {
        self.state
            .add_toast(message, crate::tui::ToastKind::Success);
    }

    /// Show an error toast notification.
    pub fn toast_error(&mut self, message: impl Into<String>) {
        self.state.add_toast(message, crate::tui::ToastKind::Error);
    }

    /// Register content for a UI slot.
    ///
    /// The content will be rendered at the designated location in the TUI.
    /// Replaces any previous content from the same plugin for this slot.
    pub fn register_slot_content(
        &mut self,
        plugin_id: impl Into<String>,
        slot: PluginSlot,
        lines: Vec<String>,
        style: SlotStyle,
    ) {
        self.state
            .register_slot_content(plugin_id, slot, lines, style);
    }

    /// Remove all slot content registered by a specific plugin.
    pub fn clear_plugin_slots(&mut self, plugin_id: &str) {
        self.state.remove_plugin_slot_content(plugin_id);
    }

    /// Get all content for a slot.
    pub fn get_slot_content(&self, slot: PluginSlot) -> Vec<&SlotContent> {
        self.state.get_slot_content(slot)
    }

    /// Register a full-screen route for this plugin.
    pub fn register_route(&mut self, route: PluginRoute) {
        self.state.register_plugin_route(route);
    }

    /// Get all registered routes.
    pub fn get_registered_routes(&self) -> Vec<&PluginRoute> {
        self.state.plugin_routes.iter().collect()
    }
}

/// A command provided by a plugin, registrable with the command palette.
///
/// Each command has a unique ID (conventionally `plugin_name:command_name`),
/// display metadata, and an execution callback that receives a `PluginApi`.
pub struct PluginCommand {
    /// Unique identifier, e.g. "theme-switcher:toggle-dark".
    pub id: String,
    /// Display name shown in the command palette.
    pub title: String,
    /// Human-readable description of what this command does.
    pub description: String,
    /// Category for grouping: "Agent", "Session", "System", or plugin name.
    pub category: String,
    /// Optional keybind hint, e.g. "Ctrl+T".
    pub keybind: Option<String>,
    /// Execution callback. Receives mutable `PluginApi` and returns an optional result string.
    ///
    /// The `for<'a>` higher-ranked bound allows the runtime to create a `PluginApi`
    /// with any lifetime and pass it to this closure.
    pub on_execute: Box<dyn for<'a> Fn(&'a mut PluginApi<'a>) -> Option<String> + Send + Sync>,
}

/// Metadata about a registered plugin, returned by `PluginRuntime::list()`.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Unique plugin identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description of the plugin.
    pub description: String,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Number of commands this plugin provides.
    pub command_count: usize,
}

/// Trait that all TUI plugins must implement.
///
/// Plugins are `Send + Sync` to allow the runtime to manage them safely.
/// The lifecycle is: `register` → `init` (when enabled) → commands available → `dispose` (when disabled).
pub trait TuiPlugin: Send + Sync {
    /// Unique machine-readable identifier for this plugin.
    fn id(&self) -> &str;

    /// Human-readable display name.
    fn name(&self) -> &str;

    /// Description shown in the plugins dialog.
    fn description(&self) -> &str;

    /// Called when the plugin is enabled/loaded.
    ///
    /// Use this for initialization: loading saved state, setting defaults, etc.
    /// Return an error string if initialization fails.
    fn init(&self, _api: &mut PluginApi<'_>) -> Result<(), String> {
        Ok(())
    }

    /// Called when the plugin is disabled/unloaded.
    ///
    /// Use this to save state and clean up resources.
    fn dispose(&self, _api: &mut PluginApi<'_>) {}

    /// Register the commands this plugin provides.
    ///
    /// Called once during plugin initialization. The returned commands are
    /// merged into the command palette and can be executed by ID.
    fn register_commands(&self) -> Vec<PluginCommand>;

    /// Register UI slot content for this plugin.
    ///
    /// Called once during plugin initialization after `init()`.
    /// Override this to inject content at designated TUI locations.
    /// The default implementation registers nothing.
    fn register_slots(&self, _api: &mut PluginApi<'_>) {}

    /// Register full-screen routes for this plugin.
    ///
    /// Called once during plugin initialization after `register_slots()`.
    /// Routes appear in the command palette and open full-screen views.
    /// The default implementation registers no routes.
    fn register_routes(&self, _api: &mut PluginApi<'_>) {}
}
