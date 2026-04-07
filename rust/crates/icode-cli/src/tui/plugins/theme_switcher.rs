use crate::tui::plugin::{PluginApi, PluginCommand, PluginSlot, SlotStyle, TuiPlugin};

pub struct ThemeSwitcherPlugin;

impl TuiPlugin for ThemeSwitcherPlugin {
    fn id(&self) -> &'static str {
        "theme-switcher"
    }

    fn name(&self) -> &'static str {
        "Theme Switcher"
    }

    fn description(&self) -> &'static str {
        "Switch between light and dark themes"
    }

    fn init(&self, api: &mut PluginApi<'_>) -> Result<(), String> {
        let saved_theme = api.kv.get("theme-switcher:last", String::new());
        if !saved_theme.is_empty() {
            api.state.set_theme(&saved_theme);
        }
        Ok(())
    }

    fn dispose(&self, api: &mut PluginApi<'_>) {
        let theme_name = if api.state.theme.is_dark() {
            "dark"
        } else {
            "light"
        };
        api.kv.set("theme-switcher:last", theme_name);
        let _ = api.kv.save();
    }

    fn register_slots(&self, api: &mut PluginApi<'_>) {
        let label = if api.state.theme.is_dark() {
            "\u{25cf} dark"
        } else {
            "\u{25cb} light"
        };
        api.register_slot_content(
            self.id(),
            PluginSlot::StatusBarRight,
            vec![label.to_string()],
            SlotStyle::Accent,
        );
    }

    fn register_commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand {
                id: "theme-switcher:toggle".to_string(),
                title: "Toggle theme".to_string(),
                description: "Switch between light and dark themes".to_string(),
                category: "System".to_string(),
                keybind: Some("Ctrl+T".to_string()),
                on_execute: Box::new(|api| {
                    let new_name = api.state.toggle_theme();
                    api.kv.set("theme-switcher:last", new_name);
                    let label = if new_name == "dark" {
                        "\u{25cf} dark"
                    } else {
                        "\u{25cb} light"
                    };
                    api.register_slot_content(
                        "theme-switcher",
                        PluginSlot::StatusBarRight,
                        vec![label.to_string()],
                        SlotStyle::Accent,
                    );
                    api.toast_success(format!("Theme switched to {new_name}"));
                    Some(format!("__theme_change__{new_name}"))
                }),
            },
            PluginCommand {
                id: "theme-switcher:dark".to_string(),
                title: "Set dark theme".to_string(),
                description: "Switch to dark theme".to_string(),
                category: "System".to_string(),
                keybind: None,
                on_execute: Box::new(|api| {
                    if !api.state.theme.is_dark() {
                        api.state.set_theme("dark");
                        api.kv.set("theme-switcher:last", "dark");
                        api.register_slot_content(
                            "theme-switcher",
                            PluginSlot::StatusBarRight,
                            vec!["\u{25cf} dark".to_string()],
                            SlotStyle::Accent,
                        );
                        api.toast_success("Switched to dark theme");
                        Some("__theme_change__dark".to_string())
                    } else {
                        api.toast("Already using dark theme");
                        None
                    }
                }),
            },
            PluginCommand {
                id: "theme-switcher:light".to_string(),
                title: "Set light theme".to_string(),
                description: "Switch to light theme".to_string(),
                category: "System".to_string(),
                keybind: None,
                on_execute: Box::new(|api| {
                    if api.state.theme.is_dark() {
                        api.state.set_theme("light");
                        api.kv.set("theme-switcher:last", "light");
                        api.register_slot_content(
                            "theme-switcher",
                            PluginSlot::StatusBarRight,
                            vec!["\u{25cb} light".to_string()],
                            SlotStyle::Accent,
                        );
                        api.toast_success("Switched to light theme");
                        Some("__theme_change__light".to_string())
                    } else {
                        api.toast("Already using light theme");
                        None
                    }
                }),
            },
        ]
    }
}
