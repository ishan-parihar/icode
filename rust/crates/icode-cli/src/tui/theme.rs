use ratatui::style::Color;

/// Theme palette matching `OpenCode`'s default theme.
/// Colors sourced from opencode.json — the canonical `OpenCode` TUI theme.
/// Uses warm orange primary (`OpenCode`'s signature color).
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // === Background ===
    pub background: Color,
    pub background_panel: Color,
    pub background_element: Color,
    pub background_hover: Color,

    // === Text ===
    pub text: Color,
    pub text_muted: Color,
    pub text_inverse: Color,

    // === Semantic ===
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // === Borders ===
    pub border: Color,
    pub border_active: Color,
    pub border_subtle: Color,

    // === Syntax (subtle) ===
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_comment: Color,
    pub syntax_type: Color,
    pub syntax_number: Color,
    pub syntax_function: Color,

    // === Diff ===
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_changed: Color,

    // === Agent colors (for message attribution) ===
    pub agent_build: Color,
    pub agent_plan: Color,
    pub agent_subagent: Color,

    // === Markdown ===
    pub heading_1: Color,
    pub heading_2: Color,
    pub heading_3: Color,
    pub code_text: Color,
    pub code_bg: Color,
    pub code_lang: Color,
    pub link: Color,
    pub link_text: Color,
    pub blockquote: Color,
    pub emph: Color,
    pub strong: Color,
    pub horizontal_rule: Color,
    pub list_item: Color,
    pub list_enum: Color,
}

impl Theme {
    /// `OpenCode` default dark theme — sourced from opencode.json.
    pub const fn dark() -> Self {
        Self {
            background: Color::Rgb(10, 10, 10),
            background_panel: Color::Rgb(20, 20, 20),
            background_element: Color::Rgb(30, 30, 30),
            background_hover: Color::Rgb(40, 40, 40),

            text: Color::Rgb(238, 238, 238),
            text_muted: Color::Rgb(128, 128, 128),
            text_inverse: Color::Rgb(10, 10, 10),

            primary: Color::Rgb(250, 178, 131),
            secondary: Color::Rgb(92, 156, 245),
            accent: Color::Rgb(157, 124, 216),
            success: Color::Rgb(127, 216, 143),
            warning: Color::Rgb(245, 167, 66),
            error: Color::Rgb(224, 108, 117),
            info: Color::Rgb(86, 182, 194),

            border: Color::Rgb(60, 60, 60),
            border_active: Color::Rgb(160, 160, 160),
            border_subtle: Color::Rgb(40, 40, 40),

            syntax_keyword: Color::Rgb(198, 120, 221),
            syntax_string: Color::Rgb(152, 195, 121),
            syntax_comment: Color::Rgb(90, 90, 90),
            syntax_type: Color::Rgb(110, 160, 224),
            syntax_number: Color::Rgb(209, 154, 102),
            syntax_function: Color::Rgb(97, 175, 239),

            diff_added: Color::Rgb(127, 216, 143),
            diff_removed: Color::Rgb(224, 108, 117),
            diff_changed: Color::Rgb(245, 167, 66),

            agent_build: Color::Rgb(127, 216, 143),
            agent_plan: Color::Rgb(250, 178, 131),
            agent_subagent: Color::Rgb(157, 124, 216),

            heading_1: Color::Rgb(250, 178, 131),
            heading_2: Color::Rgb(127, 216, 143),
            heading_3: Color::Rgb(92, 156, 245),
            code_text: Color::Rgb(238, 238, 238),
            code_bg: Color::Rgb(22, 22, 22),
            code_lang: Color::Rgb(157, 124, 216),
            link: Color::Rgb(250, 178, 131),
            link_text: Color::Rgb(86, 182, 194),
            blockquote: Color::Rgb(229, 192, 123),
            emph: Color::Rgb(229, 192, 123),
            strong: Color::Rgb(245, 167, 66),
            horizontal_rule: Color::Rgb(128, 128, 128),
            list_item: Color::Rgb(250, 178, 131),
            list_enum: Color::Rgb(86, 182, 194),
        }
    }

    pub const fn light() -> Self {
        Self {
            background: Color::Rgb(255, 255, 255),
            background_panel: Color::Rgb(250, 250, 250),
            background_element: Color::Rgb(245, 245, 245),
            background_hover: Color::Rgb(235, 235, 235),

            text: Color::Rgb(26, 26, 26),
            text_muted: Color::Rgb(138, 138, 138),
            text_inverse: Color::Rgb(255, 255, 255),

            primary: Color::Rgb(59, 125, 216),
            secondary: Color::Rgb(123, 91, 182),
            accent: Color::Rgb(214, 140, 39),
            success: Color::Rgb(61, 154, 87),
            warning: Color::Rgb(214, 140, 39),
            error: Color::Rgb(209, 56, 61),
            info: Color::Rgb(49, 135, 149),

            border: Color::Rgb(210, 210, 210),
            border_active: Color::Rgb(138, 138, 138),
            border_subtle: Color::Rgb(230, 230, 230),

            syntax_keyword: Color::Rgb(161, 66, 179),
            syntax_string: Color::Rgb(25, 116, 210),
            syntax_comment: Color::Rgb(150, 150, 150),
            syntax_type: Color::Rgb(161, 66, 179),
            syntax_number: Color::Rgb(175, 58, 35),
            syntax_function: Color::Rgb(62, 130, 230),

            diff_added: Color::Rgb(61, 154, 87),
            diff_removed: Color::Rgb(209, 56, 61),
            diff_changed: Color::Rgb(214, 140, 39),

            agent_build: Color::Rgb(61, 154, 87),
            agent_plan: Color::Rgb(59, 125, 216),
            agent_subagent: Color::Rgb(123, 91, 182),

            heading_1: Color::Rgb(214, 140, 39),
            heading_2: Color::Rgb(61, 154, 87),
            heading_3: Color::Rgb(59, 125, 216),
            code_text: Color::Rgb(26, 26, 26),
            code_bg: Color::Rgb(240, 240, 240),
            code_lang: Color::Rgb(123, 91, 182),
            link: Color::Rgb(59, 125, 216),
            link_text: Color::Rgb(49, 135, 149),
            blockquote: Color::Rgb(176, 133, 31),
            emph: Color::Rgb(176, 133, 31),
            strong: Color::Rgb(214, 140, 39),
            horizontal_rule: Color::Rgb(138, 138, 138),
            list_item: Color::Rgb(59, 125, 216),
            list_enum: Color::Rgb(49, 135, 149),
        }
    }

    /// Resolve agent color by agent name.
    pub fn agent_color(&self, agent: &str) -> Color {
        match agent {
            "build" | "builder" => self.agent_build,
            "plan" | "planner" => self.agent_plan,
            _ => self.agent_subagent,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn is_dark(&self) -> bool {
        let Color::Rgb(r, g, b) = self.background else {
            return true;
        };
        (r as u16 + g as u16 + b as u16) < 384
    }

    pub fn from_name(name: &str) -> Option<Self> {
        super::theme_loader::find_theme(name).copied()
    }

    pub fn display_name(name: &str) -> String {
        super::theme_loader::THEMES
            .iter()
            .find(|e| e.id == name).map_or_else(|| name.to_string(), |e| e.display_name.to_string())
    }
}
