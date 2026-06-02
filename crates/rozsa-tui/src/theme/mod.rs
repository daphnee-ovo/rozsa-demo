// theme/ — 颜色主题管理
//
// Internal Framework:
// theme/
// ├── mod.rs ........... 主题运行时管理 (ThemeProxy, THEME, toggle)
// └── palette.rs ....... 调色板定义 (Theme struct, dark/light)
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

pub mod palette;

pub use palette::Theme;

use std::sync::RwLock;

use ratatui::style::Color;

/// 全局主题存储 — 支持运行时切换
static THEME_STORE: RwLock<Theme> = RwLock::new(Theme::dark_const());

/// 运行时切换主题
pub fn set_theme(theme: Theme) {
    *THEME_STORE.write().unwrap() = theme;
}

/// 获取当前是否为 dark 主题
pub fn is_dark_theme() -> bool {
    THEME_STORE
        .read()
        .map(|t| matches!(t.text, Color::Rgb(212, 212, 212)))
        .unwrap_or(true)
}

/// 切换主题（dark <-> light），返回切换后的主题名
pub fn toggle_theme() -> &'static str {
    if is_dark_theme() {
        set_theme(Theme::light());
        "light"
    } else {
        set_theme(Theme::dark());
        "dark"
    }
}

/// 获取当前主题的快照（用于需要 owned Theme 的场合）
pub fn current_theme() -> Theme {
    THEME_STORE.read().unwrap().clone()
}

/// 动态主题代理 — `THEME.field` 语法保持兼容
/// 通过 Deref 返回 thread-local 缓存的引用
pub struct ThemeProxy;

impl ThemeProxy {
    /// 同步 thread-local 缓存并返回引用
    pub fn get(&self) -> Theme {
        THEME_STORE.read().unwrap().clone()
    }
}

impl std::ops::Deref for ThemeProxy {
    type Target = Theme;
    fn deref(&self) -> &Theme {
        thread_local! {
            static CACHED: std::cell::UnsafeCell<Theme> = std::cell::UnsafeCell::new(Theme::dark());
        }
        if let Ok(store) = THEME_STORE.read() {
            CACHED.with(|c| unsafe { *c.get() = store.clone() });
        }
        CACHED.with(|c| unsafe { &*c.get() })
    }
}

pub static THEME: ThemeProxy = ThemeProxy;

#[cfg(test)]
mod tests {
    use super::*;

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
        let stored = THEME_STORE.read().unwrap();
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
}
