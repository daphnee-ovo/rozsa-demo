use ratatui::layout::Rect;
use rozsa_tui::render::overlay::{
    Anchor, Margin, OverlayConfig, OverlaySize, OverlayStack, calculate_rect,
};

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
        margin: Margin {
            top: 2,
            bottom: 2,
            left: 5,
            right: 5,
        },
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
    stack.push(
        "bg".to_string(),
        OverlayConfig {
            captures_focus: false,
            ..Default::default()
        },
    );
    stack.push(
        "fg".to_string(),
        OverlayConfig {
            captures_focus: true,
            ..Default::default()
        },
    );
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
