use crate::tui::plugin::{PluginApi, PluginCommand, PluginSlot, SlotStyle, TuiPlugin};
use crate::tui::theme::Theme;
use crate::tui::theme_loader::find_theme;

pub struct ThemeSwitcherPlugin;

impl TuiPlugin for ThemeSwitcherPlugin {
    fn id(&self) -> &'static str {
        "theme-switcher"
    }

    fn name(&self) -> &'static str {
        "Theme Switcher"
    }

    fn description(&self) -> &'static str {
        "Switch between 37 OpenCode themes"
    }

    fn init(&self, api: &mut PluginApi<'_>) -> Result<(), String> {
        let saved_theme = api.kv.get("theme-switcher:last", String::new());
        if !saved_theme.is_empty() && Theme::from_name(&saved_theme).is_some() {
            api.state.set_theme(&saved_theme);
        }
        Ok(())
    }

    fn dispose(&self, api: &mut PluginApi<'_>) {
        let theme_id = current_theme_id(api);
        api.kv.set("theme-switcher:last", &theme_id);
        let _ = api.kv.save();
    }

    fn register_slots(&self, api: &mut PluginApi<'_>) {
        let theme_id = current_theme_id(api);
        let display = Theme::display_name(&theme_id);
        let icon = if Theme::from_name(&theme_id).is_none_or(|t| t.is_dark()) {
            "\u{25cf}"
        } else {
            "\u{25cb}"
        };
        api.register_slot_content(
            self.id(),
            PluginSlot::StatusBarRight,
            vec![format!("{icon} {display}")],
            SlotStyle::Accent,
        );
    }

    fn register_commands(&self) -> Vec<PluginCommand> {
        let mut commands = vec![PluginCommand {
            id: "theme-switcher:toggle".to_string(),
            title: "Cycle theme".to_string(),
            description: "Cycle to the next theme".to_string(),
            category: "System".to_string(),
            keybind: Some("Ctrl+T".to_string()),
            on_execute: Box::new(|api| {
                let new_name = api.state.toggle_theme();
                api.kv.set("theme-switcher:last", new_name);
                update_slot(api, new_name);
                api.toast_success(format!("Theme: {}", Theme::display_name(new_name)));
                None
            }),
        }];

        if let Some(current_id) = find_theme("opencode").map(|_| "opencode") {
            commands.push(PluginCommand {
                id: "theme-switcher:default".to_string(),
                title: "Set OpenCode theme".to_string(),
                description: "Switch to the default OpenCode theme".to_string(),
                category: "System".to_string(),
                keybind: None,
                on_execute: Box::new(|api| {
                    api.state.set_theme("opencode");
                    api.kv.set("theme-switcher:last", "opencode");
                    update_slot(api, "opencode");
                    api.toast_success("Theme: OpenCode");
                    None
                }),
            });
        }

        commands
    }
}

fn current_theme_id(api: &PluginApi<'_>) -> String {
    use crate::tui::theme_loader::THEMES;
    THEMES
        .iter()
        .find(|entry| entry.theme.background == api.state.theme.background)
        .map_or_else(|| "opencode".to_string(), |entry| entry.id.to_string())
}

fn update_slot(api: &mut PluginApi<'_>, theme_id: &str) {
    let display = Theme::display_name(theme_id);
    let icon = if Theme::from_name(theme_id).is_none_or(|t| t.is_dark()) {
        "\u{25cf}"
    } else {
        "\u{25cb}"
    };
    api.register_slot_content(
        "theme-switcher",
        PluginSlot::StatusBarRight,
        vec![format!("{icon} {display}")],
        SlotStyle::Accent,
    );
}
