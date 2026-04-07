use std::str::FromStr;

use syntect::highlighting::{
    Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme as SyntectTheme, ThemeItem,
    ThemeSettings,
};

use crate::tui::theme::Theme;

pub fn build_syntect_theme(tui_theme: &Theme) -> SyntectTheme {
    let default_fg = color_from_ratatui(tui_theme.text);
    let default_bg = color_from_ratatui(tui_theme.code_bg);

    let mut scopes: Vec<ThemeItem> = Vec::new();

    push_scope(
        &mut scopes,
        "keyword",
        color_from_ratatui(tui_theme.syntax_keyword),
        FontStyle::empty(),
    );

    push_scope(
        &mut scopes,
        "string",
        color_from_ratatui(tui_theme.syntax_string),
        FontStyle::empty(),
    );

    push_scope(
        &mut scopes,
        "comment",
        color_from_ratatui(tui_theme.syntax_comment),
        FontStyle::ITALIC,
    );

    push_scope(
        &mut scopes,
        "entity.name.type, support.type, storage.type",
        color_from_ratatui(tui_theme.syntax_type),
        FontStyle::empty(),
    );

    push_scope(
        &mut scopes,
        "constant.numeric",
        color_from_ratatui(tui_theme.syntax_number),
        FontStyle::empty(),
    );

    push_scope(
        &mut scopes,
        "entity.name.function",
        color_from_ratatui(tui_theme.syntax_function),
        FontStyle::empty(),
    );

    SyntectTheme {
        name: Some("icode-dynamic".to_string()),
        author: Some("icode".to_string()),
        settings: ThemeSettings {
            foreground: Some(default_fg),
            background: Some(default_bg),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

fn push_scope(
    scopes: &mut Vec<ThemeItem>,
    selector: &str,
    foreground: SynColor,
    font_style: FontStyle,
) {
    if let Ok(scope) = ScopeSelectors::from_str(selector) {
        scopes.push(ThemeItem {
            scope,
            style: StyleModifier {
                foreground: Some(foreground),
                background: None,
                font_style: Some(font_style),
            },
        });
    }
}

fn color_from_ratatui(color: ratatui::style::Color) -> SynColor {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => SynColor { r, g, b, a: 0xFF },
        _ => SynColor {
            r: 255,
            g: 255,
            b: 255,
            a: 0xFF,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_syntect_theme_has_expected_scopes() {
        let theme = Theme::dark();
        let st = build_syntect_theme(&theme);

        assert_eq!(st.name.as_deref(), Some("icode-dynamic"));
        assert_eq!(st.scopes.len(), 6);
    }

    #[test]
    fn build_syntect_theme_dark_colors() {
        let theme = Theme::dark();
        let st = build_syntect_theme(&theme);

        assert_eq!(st.settings.foreground.unwrap().r, 238);
        assert_eq!(st.settings.foreground.unwrap().g, 238);
        assert_eq!(st.settings.foreground.unwrap().b, 238);

        let keyword_fg = st.scopes[0].style.foreground.unwrap();
        assert_eq!(keyword_fg.r, 198);
        assert_eq!(keyword_fg.g, 120);
        assert_eq!(keyword_fg.b, 221);
    }

    #[test]
    fn build_syntect_theme_light_colors() {
        let theme = Theme::light();
        let st = build_syntect_theme(&theme);

        assert_eq!(st.settings.foreground.unwrap().r, 26);
        assert_eq!(st.settings.foreground.unwrap().g, 26);
        assert_eq!(st.settings.foreground.unwrap().b, 26);

        let keyword_fg = st.scopes[0].style.foreground.unwrap();
        assert_eq!(keyword_fg.r, 161);
        assert_eq!(keyword_fg.g, 66);
        assert_eq!(keyword_fg.b, 179);
    }

    #[test]
    fn build_syntect_theme_comment_is_italic() {
        let theme = Theme::dark();
        let st = build_syntect_theme(&theme);

        let comment_style = st.scopes[2].style.font_style.unwrap();
        assert!(comment_style.contains(FontStyle::ITALIC));
    }

    #[test]
    fn build_syntect_theme_background_matches_code_bg() {
        let theme = Theme::dark();
        let st = build_syntect_theme(&theme);

        let bg = st.settings.background.unwrap();
        assert_eq!(bg.r, 22);
        assert_eq!(bg.g, 22);
        assert_eq!(bg.b, 22);
    }
}
