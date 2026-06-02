use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// 快捷键管理器：合并后端绑定 + 用户自定义覆盖
#[derive(Clone, Debug, Default)]
pub struct KeybindingsManager {
    /// 合并后的绑定表（用户覆盖优先）
    merged: BTreeMap<String, Vec<String>>,
    /// 用户自定义绑定
    user_bindings: BTreeMap<String, Vec<String>>,
    /// 检测到的冲突：同一 key_id 映射到多个 action
    pub conflicts: Vec<KeybindingConflict>,
}

#[derive(Clone, Debug)]
pub struct KeybindingConflict {
    pub key_id: String,
    pub actions: Vec<String>,
}

impl KeybindingsManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 用后端传入的绑定表初始化（每次 state 更新时调用）
    pub fn update_from_backend(&mut self, backend_bindings: &BTreeMap<String, Vec<String>>) {
        self.merged = backend_bindings.clone();
        // 用户覆盖优先
        for (action, keys) in &self.user_bindings {
            self.merged.insert(action.clone(), keys.clone());
        }
        self.detect_conflicts();
    }

    /// 加载用户自定义绑定（JSON 格式）
    pub fn load_user_bindings(&mut self, user: BTreeMap<String, Vec<String>>) {
        self.user_bindings = user;
    }

    /// 获取合并后的完整绑定表（用于传递给子模块的 matches_action）
    pub fn merged_bindings(&self) -> &BTreeMap<String, Vec<String>> {
        &self.merged
    }

    /// 匹配按键是否对应某个 action
    pub fn matches(&self, key: KeyEvent, action: &str) -> bool {
        matches_action(&self.merged, key, action)
    }

    /// 获取某 action 绑定的所有 key
    pub fn keys_for(&self, action: &str) -> Option<&Vec<String>> {
        self.merged.get(action)
    }

    fn detect_conflicts(&mut self) {
        self.conflicts.clear();
        let mut key_to_actions: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (action, keys) in &self.merged {
            for key_id in keys {
                key_to_actions
                    .entry(key_id.to_lowercase())
                    .or_default()
                    .push(action.clone());
            }
        }
        for (key_id, actions) in key_to_actions {
            if actions.len() > 1 {
                self.conflicts.push(KeybindingConflict { key_id, actions });
            }
        }
    }
}

pub fn matches_action(
    bindings: &BTreeMap<String, Vec<String>>,
    key: KeyEvent,
    action: &str,
) -> bool {
    let Some(keys) = bindings.get(action) else {
        return false;
    };
    keys.iter().any(|key_id| matches_key_id(key, key_id))
}

pub fn matches_key_id(key: KeyEvent, key_id: &str) -> bool {
    let parsed = parse_key_id(key_id);
    if matches!(key.code, KeyCode::BackTab) && parsed.key == "tab" && parsed.shift {
        return true;
    }
    if parsed.key == "esc" && matches!(key.code, KeyCode::Esc) {
        return !parsed.ctrl && !parsed.alt && !parsed.shift;
    }
    key_name(key.code) == parsed.key
        && key.modifiers.contains(KeyModifiers::CONTROL) == parsed.ctrl
        && key.modifiers.contains(KeyModifiers::ALT) == parsed.alt
        && key.modifiers.contains(KeyModifiers::SHIFT) == parsed.shift
}

struct ParsedKey {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

fn parse_key_id(key_id: &str) -> ParsedKey {
    let parts = key_id.split('+').map(str::to_lowercase).collect::<Vec<_>>();
    let key = parts.last().cloned().unwrap_or_default();
    ParsedKey {
        key,
        ctrl: parts.iter().any(|part| part == "ctrl"),
        alt: parts.iter().any(|part| part == "alt"),
        shift: parts.iter().any(|part| part == "shift"),
    }
}

fn key_name(code: KeyCode) -> String {
    match code {
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "tab".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Esc => "escape".to_string(),
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) => ch.to_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(action: &str, keys: Vec<&str>) -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([(
            action.to_string(),
            keys.into_iter().map(str::to_string).collect(),
        )])
    }

    #[test]
    fn matches_control_key_action() {
        let map = bindings("app.model.cycleForward", vec!["ctrl+p"]);
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(matches_action(&map, key, "app.model.cycleForward"));
    }

    #[test]
    fn matches_shift_tab_from_backtab_event() {
        let map = bindings("app.editMode.cycle", vec!["shift+tab"]);
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert!(matches_action(&map, key, "app.editMode.cycle"));
    }

    #[test]
    fn matches_escape_aliases() {
        assert!(matches_key_id(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            "escape"
        ));
        assert!(matches_key_id(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            "esc"
        ));
    }
}
