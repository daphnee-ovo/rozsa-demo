use ratatui::style::Color;
use rozsa_tui::theme::{set_theme, Theme};

#[test]
fn dark_theme_default() {
    let theme = Theme::default();
    assert_eq!(theme.text, Color::Rgb(212, 212, 212));
}

#[test]
fn light_theme_colors_differ() {
    let dark = Theme::dark();
    let light = Theme::light();
    assert_ne!(dark.text, light.text);
    assert_ne!(dark.accent, light.accent);
    assert_ne!(dark.selected_bg, light.selected_bg);
}

#[test]
fn set_theme_updates_store() {
    let light = Theme::light();
    set_theme(light.clone());
    // 通过 current_theme() 校验，避免依赖私有 store
    let stored = rozsa_tui::theme::current_theme();
    assert_eq!(stored.text, light.text);
}

#[test]
fn dark_and_light_have_all_fields() {
    let dark = Theme::dark();
    let light = Theme::light();
    assert!(matches!(dark.accent, Color::Rgb(_, _, _)));
    assert!(matches!(light.accent, Color::Rgb(_, _, _)));
    assert!(matches!(dark.md_code, Color::Rgb(_, _, _)));
    assert!(matches!(light.md_code, Color::Rgb(_, _, _)));
}
