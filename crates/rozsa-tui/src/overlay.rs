// overlay.rs — Overlay 定位与焦点管理
//
// Internal Framework:
// overlay.rs
// ├── Anchor         — 9锚点枚举（4角 + 4边 + 中心）
// ├── OverlaySize    — 尺寸定义（固定/百分比）
// ├── OverlayConfig  — overlay 配置
// ├── OverlayStack   — 焦点栈（LIFO）
// └── calculate_rect() — 根据锚点和尺寸计算最终 Rect
//
// Related Docs:
// - [TS overlay](../../../packages/tui/src/tui.ts)

use ratatui::layout::Rect;

/// 9 锚点定位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// 尺寸：固定值或百分比
#[derive(Debug, Clone, Copy)]
pub enum OverlaySize {
    Fixed(u16),
    Percent(u16),
}

impl OverlaySize {
    pub fn resolve(self, available: u16) -> u16 {
        match self {
            Self::Fixed(v) => v.min(available),
            Self::Percent(p) => (available as u32 * p.min(100) as u32 / 100) as u16,
        }
    }
}

/// Overlay 配置
#[derive(Debug, Clone)]
pub struct OverlayConfig {
    pub anchor: Anchor,
    pub width: OverlaySize,
    pub height: OverlaySize,
    pub margin: Margin,
    /// 最小终端宽度（低于此值自动隐藏）
    pub min_terminal_width: u16,
    /// 最小终端高度
    pub min_terminal_height: u16,
    /// 是否捕获焦点
    pub captures_focus: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Margin {
    pub top: u16,
    pub bottom: u16,
    pub left: u16,
    pub right: u16,
}

impl Margin {
    pub fn uniform(m: u16) -> Self {
        Self { top: m, bottom: m, left: m, right: m }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            anchor: Anchor::Center,
            width: OverlaySize::Percent(80),
            height: OverlaySize::Percent(80),
            margin: Margin::default(),
            min_terminal_width: 40,
            min_terminal_height: 10,
            captures_focus: true,
        }
    }
}

/// 根据配置计算最终渲染区域
pub fn calculate_rect(config: &OverlayConfig, viewport: Rect) -> Option<Rect> {
    // 条件可见性检查
    if viewport.width < config.min_terminal_width || viewport.height < config.min_terminal_height {
        return None;
    }

    let available_w = viewport.width.saturating_sub(config.margin.left + config.margin.right);
    let available_h = viewport.height.saturating_sub(config.margin.top + config.margin.bottom);

    let w = config.width.resolve(available_w);
    let h = config.height.resolve(available_h);

    if w == 0 || h == 0 {
        return None;
    }

    let (x, y) = anchor_position(config.anchor, viewport, w, h, &config.margin);

    Some(Rect::new(x, y, w, h))
}

fn anchor_position(anchor: Anchor, viewport: Rect, w: u16, h: u16, margin: &Margin) -> (u16, u16) {
    let left = viewport.x + margin.left;
    let top = viewport.y + margin.top;
    let right_edge = viewport.x + viewport.width.saturating_sub(margin.right);
    let bottom_edge = viewport.y + viewport.height.saturating_sub(margin.bottom);

    let center_x = left + (right_edge.saturating_sub(left).saturating_sub(w)) / 2;
    let center_y = top + (bottom_edge.saturating_sub(top).saturating_sub(h)) / 2;

    match anchor {
        Anchor::TopLeft => (left, top),
        Anchor::TopCenter => (center_x, top),
        Anchor::TopRight => (right_edge.saturating_sub(w), top),
        Anchor::CenterLeft => (left, center_y),
        Anchor::Center => (center_x, center_y),
        Anchor::CenterRight => (right_edge.saturating_sub(w), center_y),
        Anchor::BottomLeft => (left, bottom_edge.saturating_sub(h)),
        Anchor::BottomCenter => (center_x, bottom_edge.saturating_sub(h)),
        Anchor::BottomRight => (right_edge.saturating_sub(w), bottom_edge.saturating_sub(h)),
    }
}

/// Overlay 焦点栈（LIFO）— 管理 permission/dialog/graph 等浮层的焦点优先级
#[derive(Clone, Debug, Default)]
pub struct OverlayStack {
    stack: Vec<OverlayHandle>,
}

#[derive(Clone, Debug)]
pub struct OverlayHandle {
    pub id: String,
    pub config: OverlayConfig,
    pub visible: bool,
}

impl OverlayStack {
    pub fn push(&mut self, id: String, config: OverlayConfig) {
        self.stack.push(OverlayHandle {
            id,
            config,
            visible: true,
        });
    }

    pub fn pop(&mut self) -> Option<OverlayHandle> {
        self.stack.pop()
    }

    pub fn remove(&mut self, id: &str) {
        self.stack.retain(|h| h.id != id);
    }

    pub fn top(&self) -> Option<&OverlayHandle> {
        self.stack.last()
    }

    pub fn set_visible(&mut self, id: &str, visible: bool) {
        if let Some(h) = self.stack.iter_mut().find(|h| h.id == id) {
            h.visible = visible;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// 当前有焦点捕获的 overlay
    pub fn focus_target(&self) -> Option<&OverlayHandle> {
        self.stack.iter().rev().find(|h| h.visible && h.config.captures_focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_anchor_calculation() {
        let config = OverlayConfig {
            anchor: Anchor::Center,
            width: OverlaySize::Fixed(40),
            height: OverlaySize::Fixed(10),
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 100, 40);
        let rect = calculate_rect(&config, viewport).unwrap();
        assert_eq!(rect.width, 40);
        assert_eq!(rect.height, 10);
        assert_eq!(rect.x, 30); // (100 - 40) / 2
        assert_eq!(rect.y, 15); // (40 - 10) / 2
    }

    #[test]
    fn percent_sizing() {
        let config = OverlayConfig {
            anchor: Anchor::Center,
            width: OverlaySize::Percent(50),
            height: OverlaySize::Percent(50),
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 80, 24);
        let rect = calculate_rect(&config, viewport).unwrap();
        assert_eq!(rect.width, 40);
        assert_eq!(rect.height, 12);
    }

    #[test]
    fn too_small_viewport_returns_none() {
        let config = OverlayConfig {
            min_terminal_width: 80,
            min_terminal_height: 24,
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 30, 10);
        assert!(calculate_rect(&config, viewport).is_none());
    }

    #[test]
    fn margin_reduces_available_space() {
        let config = OverlayConfig {
            anchor: Anchor::TopLeft,
            width: OverlaySize::Percent(100),
            height: OverlaySize::Percent(100),
            margin: Margin { top: 2, bottom: 2, left: 5, right: 5 },
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 80, 24);
        let rect = calculate_rect(&config, viewport).unwrap();
        assert_eq!(rect.x, 5);
        assert_eq!(rect.y, 2);
        assert_eq!(rect.width, 70); // 80 - 5 - 5
        assert_eq!(rect.height, 20); // 24 - 2 - 2
    }

    #[test]
    fn overlay_stack_lifo() {
        let mut stack = OverlayStack::default();
        stack.push("a".to_string(), OverlayConfig::default());
        stack.push("b".to_string(), OverlayConfig::default());
        assert_eq!(stack.top().unwrap().id, "b");
        stack.pop();
        assert_eq!(stack.top().unwrap().id, "a");
    }

    #[test]
    fn overlay_stack_remove_by_id() {
        let mut stack = OverlayStack::default();
        stack.push("a".to_string(), OverlayConfig::default());
        stack.push("b".to_string(), OverlayConfig::default());
        stack.push("c".to_string(), OverlayConfig::default());
        stack.remove("b");
        assert_eq!(stack.top().unwrap().id, "c");
        stack.pop();
        assert_eq!(stack.top().unwrap().id, "a");
    }

    #[test]
    fn overlay_stack_is_empty() {
        let mut stack = OverlayStack::default();
        assert!(stack.is_empty());
        stack.push("x".to_string(), OverlayConfig::default());
        assert!(!stack.is_empty());
        stack.pop();
        assert!(stack.is_empty());
    }

    #[test]
    fn overlay_stack_set_visible() {
        let mut stack = OverlayStack::default();
        stack.push("a".to_string(), OverlayConfig::default());
        assert!(stack.top().unwrap().visible);
        stack.set_visible("a", false);
        assert!(!stack.top().unwrap().visible);
    }

    #[test]
    fn overlay_stack_focus_target_skips_hidden() {
        let mut stack = OverlayStack::default();
        stack.push("a".to_string(), OverlayConfig::default());
        stack.push("b".to_string(), OverlayConfig::default());
        // Hide top overlay — focus should fall through to "a"
        stack.set_visible("b", false);
        assert_eq!(stack.focus_target().unwrap().id, "a");
    }

    #[test]
    fn overlay_stack_focus_target_respects_captures_focus() {
        let mut stack = OverlayStack::default();
        stack.push("bg".to_string(), OverlayConfig {
            captures_focus: false,
            ..Default::default()
        });
        stack.push("fg".to_string(), OverlayConfig {
            captures_focus: true,
            ..Default::default()
        });
        assert_eq!(stack.focus_target().unwrap().id, "fg");
        // Remove "fg" — "bg" doesn't capture focus, so no focus target
        stack.pop();
        assert!(stack.focus_target().is_none());
    }

    #[test]
    fn overlay_size_resolve_fixed_clamps() {
        // Fixed(100) with available 60 → clamps to 60
        assert_eq!(OverlaySize::Fixed(100).resolve(60), 60);
        assert_eq!(OverlaySize::Fixed(30).resolve(60), 30);
    }

    #[test]
    fn overlay_size_resolve_percent_clamps() {
        assert_eq!(OverlaySize::Percent(50).resolve(80), 40);
        // Over 100% still clamps
        assert_eq!(OverlaySize::Percent(150).resolve(80), 80);
    }

    #[test]
    fn anchor_top_left_position() {
        let config = OverlayConfig {
            anchor: Anchor::TopLeft,
            width: OverlaySize::Fixed(20),
            height: OverlaySize::Fixed(5),
            margin: Margin::uniform(2),
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 80, 24);
        let rect = calculate_rect(&config, viewport).unwrap();
        assert_eq!(rect.x, 2);
        assert_eq!(rect.y, 2);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn anchor_bottom_right_position() {
        let config = OverlayConfig {
            anchor: Anchor::BottomRight,
            width: OverlaySize::Fixed(20),
            height: OverlaySize::Fixed(5),
            margin: Margin::default(),
            ..Default::default()
        };
        let viewport = Rect::new(0, 0, 80, 24);
        let rect = calculate_rect(&config, viewport).unwrap();
        assert_eq!(rect.x, 60); // 80 - 20
        assert_eq!(rect.y, 19); // 24 - 5
    }
}
