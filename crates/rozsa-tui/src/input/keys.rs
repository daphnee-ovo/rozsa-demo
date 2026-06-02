// input/keys.rs — 键盘输入操作实现（选区、折叠、grapheme 工具、文本编辑）
//
// 内部结构:
// keys.rs
// ├── impl InputState (selection)
// │   ├── selection_range()
// │   ├── selected_text()
// │   ├── delete_selection()
// │   └── clear_selection()
// ├── impl InputState (fold)
// │   ├── is_folded()
// │   ├── fold_range()
// │   ├── unfold_at()
// │   └── visible_lines()
// ├── grapheme helpers
// │   ├── grapheme_count()
// │   ├── grapheme_take()
// │   ├── grapheme_skip()
// │   ├── grapheme_to_byte_offset()
// │   └── is_word_char()
// ├── impl InputState (core editing)
// │   ├── set_text() / text() / is_empty() / clear()
// │   ├── current_line() / push_undo() / undo()
// ├── impl EditorComponent for InputState
// ├── pub fn handle_key() — main key handler
// ├── key operation functions
// │   ├── insert_char() / find_atomic_span() / find_atomic_span_forward()
// │   ├── delete_char_backward() / delete_char_forward()
// │   ├── delete_word_backward_text() / delete_word_forward_text()
// │   ├── insert_text_at_cursor() / delete_chars_backward()
// │   ├── jump_to_char() / find_fold_end()
// │   ├── move_word_forward() / move_word_backward()
// │   ├── cursor_char_index() / is_autocomplete_context()
// │   ├── get_next_subagent() / handle_dialog_key()
// │   ├── open_external_editor() / suspend_process()
// │   └── take_images()
// └── tests
//
// 相关文档:
// - [SPEC](../../../../.dev-doc/refactor/tui/SPEC.md)

use std::{env, error::Error, fs, io::Write, process::Command};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

use serde_json::Value;

use crate::{
    app::{AppState, DialogState},
    components::editor::{EditorComponent, EditorMode},
    components::graph::handle_graph_key,
    keymap::matches_action,
    kill_ring::{LastAction, PushOpts},
    components::model_selector::handle_model_selector_key,
    components::permission::handle_permission_key,
    protocol::{send, ClientMessage, ImagePayload},
    components::session_selector::handle_session_selector_key,
};

use super::{AtomicSpan, InputState, JumpDirection, SelectionAnchor, Writer};

impl InputState {
    /// 获取选区范围：(start_row, start_col, end_row, end_col)，保证 start <= end
    pub fn selection_range(&self) -> Option<(usize, usize, usize, usize)> {
        let anchor = self.selection_anchor.as_ref()?;
        let (sr, sc, er, ec) = if (anchor.row, anchor.col) <= (self.cursor_row, self.cursor_col) {
            (anchor.row, anchor.col, self.cursor_row, self.cursor_col)
        } else {
            (self.cursor_row, self.cursor_col, anchor.row, anchor.col)
        };
        if sr == er && sc == ec {
            return None;
        }
        Some((sr, sc, er, ec))
    }

    /// 获取选区内的文本
    pub fn selected_text(&self) -> Option<String> {
        let (sr, sc, er, ec) = self.selection_range()?;
        if sr == er {
            let line = &self.lines[sr];
            let start = grapheme_to_byte_offset(line, sc);
            let end = grapheme_to_byte_offset(line, ec);
            return Some(line[start..end].to_string());
        }
        let mut result = String::new();
        // First line
        let first = &self.lines[sr];
        let start = grapheme_to_byte_offset(first, sc);
        result.push_str(&first[start..]);
        // Middle lines
        for row in (sr + 1)..er {
            result.push('\n');
            result.push_str(&self.lines[row]);
        }
        // Last line
        result.push('\n');
        let last = &self.lines[er];
        let end = grapheme_to_byte_offset(last, ec);
        result.push_str(&last[..end]);
        Some(result)
    }

    /// 删除选区内的文本并清除选区
    pub fn delete_selection(&mut self) -> Option<String> {
        let (sr, sc, er, ec) = self.selection_range()?;
        let deleted = self.selected_text()?;
        self.push_undo();
        if sr == er {
            let line = &mut self.lines[sr];
            let start = grapheme_to_byte_offset(line, sc);
            let end = grapheme_to_byte_offset(line, ec);
            line.drain(start..end);
        } else {
            let first = &self.lines[sr];
            let keep_start = grapheme_to_byte_offset(first, sc);
            let last = &self.lines[er];
            let keep_end_offset = grapheme_to_byte_offset(last, ec);
            let tail = last[keep_end_offset..].to_string();
            let first_line = &mut self.lines[sr];
            first_line.truncate(keep_start);
            first_line.push_str(&tail);
            // Remove lines between
            self.lines.drain((sr + 1)..=er);
        }
        self.cursor_row = sr;
        self.cursor_col = sc;
        self.selection_anchor = None;
        Some(deleted)
    }

    /// 清除选区（不删除文本）
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }
}

impl InputState {
    /// 判断某行是否在折叠范围内（非首行）
    pub fn is_folded(&self, row: usize) -> bool {
        self.folded_ranges
            .iter()
            .any(|(start, end)| row > *start && row <= *end)
    }

    /// 折叠从 start 到 end 行（包含）
    pub fn fold_range(&mut self, start: usize, end: usize) {
        if start < end && end < self.lines.len() {
            self.folded_ranges.push((start, end));
        }
    }

    /// 展开包含指定行的折叠范围
    pub fn unfold_at(&mut self, row: usize) {
        self.folded_ranges
            .retain(|(s, e)| !(row >= *s && row <= *e));
    }

    /// 获取显示行（折叠后的可见行列表）
    pub fn visible_lines(&self) -> Vec<(usize, &str)> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < self.lines.len() {
            if let Some((_start, end)) = self.folded_ranges.iter().find(|(s, _)| *s == i) {
                result.push((i, "...")); // 折叠行显示占位
                i = *end + 1;
            } else if self.is_folded(i) {
                i += 1;
            } else {
                result.push((i, self.lines[i].as_str()));
                i += 1;
            }
        }
        result
    }
}

/// 计算一行的 grapheme cluster 数量
pub fn grapheme_count(s: &str) -> usize {
    s.graphemes(true).count()
}

/// 取行的前 n 个 grapheme
pub fn grapheme_take(s: &str, n: usize) -> String {
    s.graphemes(true).take(n).collect()
}

/// 跳过前 n 个 grapheme 后剩余部分
pub fn grapheme_skip(s: &str, n: usize) -> String {
    s.graphemes(true).skip(n).collect()
}

/// 将 grapheme 位置转换为字节偏移
pub fn grapheme_to_byte_offset(s: &str, pos: usize) -> usize {
    s.grapheme_indices(true)
        .nth(pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 判断字符是否为"单词字符"（用于标点感知的 word movement）
pub fn is_word_char(g: &str) -> bool {
    let ch = g.chars().next().unwrap_or(' ');
    ch.is_alphanumeric() || ch == '_'
}

pub fn undo(input: &mut InputState) {
    if let Some(snapshot) = input.undo_stack.pop() {
        input.lines = snapshot.lines;
        input.cursor_row = snapshot.cursor_row;
        input.cursor_col = snapshot.cursor_col;
        input.last_action = None;
        input.history_index = None;
    }
}

/// EditorComponent trait 实现 — 将 InputState 作为默认编辑器
impl EditorComponent for InputState {
    fn text(&self) -> String {
        self.text()
    }

    fn set_text(&mut self, text: String) {
        self.set_text(text);
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                if ch.is_whitespace() || self.last_action != Some(LastAction::TypeWord) {
                    self.push_undo();
                }
                self.last_action = Some(LastAction::TypeWord);
                insert_char(self, ch);
                true
            }
            KeyCode::Backspace => {
                self.push_undo();
                self.last_action = None;
                delete_char_backward(self);
                true
            }
            KeyCode::Delete => {
                self.push_undo();
                self.last_action = None;
                delete_char_forward(self);
                true
            }
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                undo(self);
                true
            }
            _ => false,
        }
    }

    fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    fn lines(&self) -> &[String] {
        &self.lines
    }

    fn mode(&self) -> EditorMode {
        EditorMode::Default
    }
}

pub fn cursor_char_index(input: &InputState) -> usize {
    let preceding_rows: usize = input
        .lines
        .iter()
        .take(input.cursor_row)
        .map(|line| grapheme_count(line) + 1)
        .sum();
    preceding_rows + input.cursor_col
}

pub fn is_autocomplete_context(text: &str, cursor: usize) -> bool {
    if text.starts_with('/') {
        return true;
    }
    let before_cursor: String = text.chars().take(cursor).collect();
    let token = before_cursor
        .split_whitespace()
        .last()
        .unwrap_or(before_cursor.as_str());
    token.starts_with('@')
}

/// 编辑操作后，偏移同一行中位于 `after_col` 之后的 atomic spans。
fn shift_atomic_spans(input: &mut InputState, row: usize, after_col: usize, delta: isize) {
    for span in &mut input.atomic_spans {
        if span.row == row && span.col_start >= after_col {
            span.col_start = (span.col_start as isize + delta).max(0) as usize;
        }
    }
}

/// 移除指定位置的 atomic span。
fn remove_atomic_span(input: &mut InputState, row: usize, col_start: usize) {
    input.atomic_spans.retain(|s| !(s.row == row && s.col_start == col_start));
}

pub fn insert_char(input: &mut InputState, ch: char) {
    if find_atomic_span(input).is_some() {
        return;
    }
    let line = &mut input.lines[input.cursor_row];
    let byte_offset = grapheme_to_byte_offset(line, input.cursor_col);
    line.insert(byte_offset, ch);
    input.cursor_col += 1;
    shift_atomic_spans(input, input.cursor_row, input.cursor_col - 1, 1);
}

/// 查找光标所在位置的原子 span（光标在 span 内部或右边界时返回）。
/// 供删除、插入、光标移动等操作使用。
pub fn find_atomic_span(input: &InputState) -> Option<&AtomicSpan> {
    input.atomic_spans.iter().find(|span| {
        span.row == input.cursor_row
            && input.cursor_col > span.col_start
            && input.cursor_col <= span.col_start + span.col_len
    })
}

/// 查找光标紧邻右侧的原子 span（用于 delete forward）。
pub fn find_atomic_span_forward(input: &InputState) -> Option<&AtomicSpan> {
    input.atomic_spans.iter().find(|span| {
        span.row == input.cursor_row && input.cursor_col == span.col_start
    })
}

pub fn delete_char_backward(input: &mut InputState) {
    if input.cursor_col == 0 {
        if input.cursor_row == 0 {
            return;
        }
        let prev_len = grapheme_count(&input.lines[input.cursor_row - 1]);
        let removed = input.lines.remove(input.cursor_row);
        input.cursor_row -= 1;
        input.cursor_col = grapheme_count(&input.lines[input.cursor_row]);
        input.lines[input.cursor_row].push_str(&removed);
        // 合并行时调整 atomic spans：被合并行的 spans 移到上一行
        let merged_row = input.cursor_row + 1;
        for span in &mut input.atomic_spans {
            if span.row == merged_row {
                span.row = input.cursor_row;
                span.col_start += prev_len;
            } else if span.row > merged_row {
                span.row -= 1;
            }
        }
        return;
    }
    // 原子 span 整体删除
    if let Some(span) = find_atomic_span(input) {
        let start_col = span.col_start;
        let span_len = span.col_len;
        let line = &mut input.lines[input.cursor_row];
        let byte_start = grapheme_to_byte_offset(line, start_col);
        let byte_end = grapheme_to_byte_offset(line, start_col + span_len);
        line.drain(byte_start..byte_end);
        input.cursor_col = start_col;
        remove_atomic_span(input, input.cursor_row, start_col);
        shift_atomic_spans(input, input.cursor_row, start_col, -(span_len as isize));
        return;
    }
    let line = &mut input.lines[input.cursor_row];
    let target = input.cursor_col - 1;
    let start = grapheme_to_byte_offset(line, target);
    let end = grapheme_to_byte_offset(line, input.cursor_col);
    line.drain(start..end);
    input.cursor_col = target;
    shift_atomic_spans(input, input.cursor_row, target, -1);
}

pub fn delete_char_forward(input: &mut InputState) {
    let line_len = grapheme_count(&input.lines[input.cursor_row]);
    if input.cursor_col >= line_len {
        if input.cursor_row + 1 < input.lines.len() {
            let next_row = input.cursor_row + 1;
            let cur_len = line_len;
            let next = input.lines.remove(next_row);
            input.lines[input.cursor_row].push_str(&next);
            // 合并行时调整 atomic spans
            for span in &mut input.atomic_spans {
                if span.row == next_row {
                    span.row = input.cursor_row;
                    span.col_start += cur_len;
                } else if span.row > next_row {
                    span.row -= 1;
                }
            }
        }
        return;
    }
    // 原子 span 整体删除
    if let Some(span) = find_atomic_span_forward(input) {
        let start_col = span.col_start;
        let span_len = span.col_len;
        let line = &mut input.lines[input.cursor_row];
        let byte_start = grapheme_to_byte_offset(line, start_col);
        let byte_end = grapheme_to_byte_offset(line, start_col + span_len);
        line.drain(byte_start..byte_end);
        remove_atomic_span(input, input.cursor_row, start_col);
        shift_atomic_spans(input, input.cursor_row, start_col, -(span_len as isize));
        return;
    }
    let col = input.cursor_col;
    let line = &mut input.lines[input.cursor_row];
    let start = grapheme_to_byte_offset(line, col);
    let end = grapheme_to_byte_offset(line, col + 1);
    line.drain(start..end);
    shift_atomic_spans(input, input.cursor_row, col, -1);
}

/// 删除光标前一个 word（标点感知），返回被删除的文本
pub fn delete_word_backward_text(input: &mut InputState) -> String {
    if input.cursor_col == 0 {
        return String::new();
    }
    let line = &input.lines[input.cursor_row];
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let mut pos = input.cursor_col;
    // 跳过前导空白
    while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
        pos -= 1;
    }
    if pos > 0 {
        let is_word = is_word_char(graphemes[pos - 1]);
        // 跳过同类字符（word 或标点）
        while pos > 0 && is_word_char(graphemes[pos - 1]) == is_word && !graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }
    }
    let deleted: String = graphemes[pos..input.cursor_col].concat();
    let new_line: String = graphemes[..pos]
        .iter()
        .chain(&graphemes[input.cursor_col..])
        .copied()
        .collect();
    input.lines[input.cursor_row] = new_line;
    input.cursor_col = pos;
    deleted
}

/// 删除光标后一个 word（标点感知），返回被删除的文本
pub fn delete_word_forward_text(input: &mut InputState) -> String {
    let line = &input.lines[input.cursor_row];
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let len = graphemes.len();
    if input.cursor_col >= len {
        return String::new();
    }
    let mut pos = input.cursor_col;
    // 跳过同类字符
    let is_word = is_word_char(graphemes[pos]);
    while pos < len && is_word_char(graphemes[pos]) == is_word && !graphemes[pos].chars().all(|c| c.is_whitespace()) {
        pos += 1;
    }
    // 跳过尾随空白
    while pos < len && graphemes[pos].chars().all(|c| c.is_whitespace()) {
        pos += 1;
    }
    let deleted: String = graphemes[input.cursor_col..pos].concat();
    let new_line: String = graphemes[..input.cursor_col]
        .iter()
        .chain(&graphemes[pos..])
        .copied()
        .collect();
    input.lines[input.cursor_row] = new_line;
    deleted
}

/// 在光标位置插入文本，返回插入的 grapheme 数
pub fn insert_text_at_cursor(input: &mut InputState, text: &str) -> usize {
    let normalized = crate::normalize_newlines(text);
    let mut count = 0;
    for g in normalized.graphemes(true) {
        if g == "\n" {
            let line = &mut input.lines[input.cursor_row];
            let tail = grapheme_skip(line, input.cursor_col);
            *line = grapheme_take(line, input.cursor_col);
            input.cursor_row += 1;
            input.lines.insert(input.cursor_row, tail);
            input.cursor_col = 0;
        } else {
            let line = &mut input.lines[input.cursor_row];
            let byte_offset = grapheme_to_byte_offset(line, input.cursor_col);
            line.insert_str(byte_offset, g);
            input.cursor_col += 1;
        }
        count += 1;
    }
    count
}

/// 从光标位置向前删除 n 个 grapheme cluster
pub fn delete_chars_backward(input: &mut InputState, n: usize) {
    for _ in 0..n {
        delete_char_backward(input);
    }
}

/// 跳转到当前行中目标字符的下一个/上一个位置
pub fn jump_to_char(input: &mut InputState, target: char, direction: JumpDirection) {
    let line = &input.lines[input.cursor_row];
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let target_lower = target.to_lowercase().next().unwrap_or(target);

    match direction {
        JumpDirection::Forward => {
            for i in (input.cursor_col + 1)..graphemes.len() {
                if let Some(ch) = graphemes[i].chars().next() {
                    if ch.to_lowercase().next().unwrap_or(ch) == target_lower {
                        input.cursor_col = i;
                        return;
                    }
                }
            }
        }
        JumpDirection::Backward => {
            for i in (0..input.cursor_col).rev() {
                if let Some(ch) = graphemes[i].chars().next() {
                    if ch.to_lowercase().next().unwrap_or(ch) == target_lower {
                        input.cursor_col = i;
                        return;
                    }
                }
            }
        }
    }
}

/// 从 row 开始，找到缩进块结束的行号（用于折叠）
pub fn find_fold_end(input: &InputState, row: usize) -> usize {
    if row >= input.lines.len() {
        return row;
    }
    let base_indent = input.lines[row]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    let mut end = row;
    for i in (row + 1)..input.lines.len() {
        let line = &input.lines[i];
        if line.trim().is_empty() {
            end = i;
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent > base_indent {
            end = i;
        } else {
            break;
        }
    }
    end
}

pub fn get_next_subagent(ui: &crate::protocol::NativeUiState, direction: i32) -> Option<String> {
    let runtime_state = ui.runtime_state.as_ref()?;
    let subagents = runtime_state.get("activeSubagents")?.as_array()?;
    if subagents.is_empty() {
        return None;
    }

    let mut ids: Vec<String> = vec!["main".to_string()];
    for agent in subagents {
        if let Some(id) = agent.as_str() {
            ids.push(id.to_string());
        } else if let Some(obj) = agent.as_object() {
            if let Some(Value::String(id)) = obj.get("id") {
                ids.push(id.clone());
            }
        }
    }

    if ids.len() <= 1 {
        return None;
    }

    let current_id = runtime_state
        .get("currentAgentId")
        .and_then(|v| v.as_str())
        .unwrap_or("main");

    let current_idx = ids.iter().position(|id| id == current_id).unwrap_or(0);
    let len = ids.len() as i32;
    let next_idx = ((current_idx as i32 + direction).rem_euclid(len)) as usize;

    Some(ids[next_idx].clone())
}

/// 前进一个 word（标点感知）
pub fn move_word_forward(input: &mut InputState) {
    input.last_action = None;
    let line = &input.lines[input.cursor_row];
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let len = graphemes.len();
    let mut pos = input.cursor_col;
    if pos < len {
        let starting_is_word = is_word_char(graphemes[pos]);
        // 跳过同类字符
        while pos < len && is_word_char(graphemes[pos]) == starting_is_word && !graphemes[pos].chars().all(|c| c.is_whitespace()) {
            pos += 1;
        }
    }
    // 跳过空白
    while pos < len && graphemes[pos].chars().all(|c| c.is_whitespace()) {
        pos += 1;
    }
    input.cursor_col = pos;
}

/// 后退一个 word（标点感知）
pub fn move_word_backward(input: &mut InputState) {
    input.last_action = None;
    let line = &input.lines[input.cursor_row];
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let mut pos = input.cursor_col;
    // 跳过前导空白
    while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
        pos -= 1;
    }
    if pos > 0 {
        let target_is_word = is_word_char(graphemes[pos - 1]);
        // 跳过同类字符
        while pos > 0 && is_word_char(graphemes[pos - 1]) == target_is_word && !graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }
    }
    input.cursor_col = pos;
}

pub fn handle_key(
    key: KeyEvent,
    state: &mut AppState,
    writer: &Writer,
    input: &mut InputState,
) -> Result<(), Box<dyn Error>> {
    // 通过 KeybindingsManager 获取合并后的绑定表（含用户覆盖）
    let keybindings = state.keybindings_manager.merged_bindings().clone();

    if let Some(permission_state) = state.permission.take() {
        let next = handle_permission_key(key, permission_state, writer, &keybindings)?;
        state.permission = next;
        return Ok(());
    }

    if let Some(graph_state) = state.graph.take() {
        let next = handle_graph_key(key, graph_state, &keybindings);
        state.graph = next;
        return Ok(());
    }

    if let Some(sel_state) = state.session_selector.take() {
        let next = handle_session_selector_key(key, sel_state, writer)?;
        state.session_selector = next;
        return Ok(());
    }

    if let Some(sel_state) = state.model_selector.take() {
        let next = handle_model_selector_key(key, sel_state, writer)?;
        state.model_selector = next;
        return Ok(());
    }

    // Dialog 优先级高于 autocomplete — dialog 存在时直接处理 dialog 按键
    if let Some(dialog_state) = state.dialog.clone() {
        state.autocomplete = None;
        return handle_dialog_key(key, state, writer, dialog_state, &keybindings);
    }

    if let Some(ac_state) = state.autocomplete.take() {
        let current_text = input.text();
        let current_cursor = cursor_char_index(input);
        let (next, action) = crate::components::autocomplete::handle_autocomplete_key(key, ac_state.clone());
        state.autocomplete = next;
        match action {
            crate::components::autocomplete::AutocompleteAction::ApplyAndSubmit => {
                // 应用补全并提交（slash command + Enter）
                let (new_text, _) =
                    crate::components::autocomplete::apply_completion(&current_text, current_cursor, &ac_state);
                let text = new_text.trim().to_string();
                if !text.is_empty() {
                    input.history.push(text.clone());
                    input.clear();
                    input.history_index = None;
                    let images = take_images(&mut state.attached_images);
                    send(
                        writer,
                        &ClientMessage::Submit {
                            text: &text,
                            images,
                        },
                    )?;
                }
                return Ok(());
            }
            crate::components::autocomplete::AutocompleteAction::ApplyAndEdit => {
                // 应用补全，继续编辑（Tab 或 @file + Enter）
                let (new_text, new_cursor) =
                    crate::components::autocomplete::apply_completion(&current_text, current_cursor, &ac_state);
                input.set_text(new_text);
                input.cursor_col = new_cursor;
                // 补全刚应用成功 → 标记有效，不再发 autocomplete 请求
                // （避免 TS 端对已完成命令返回空导致高亮消失）
                let text = input.text();
                if is_autocomplete_context(&text, cursor_char_index(input)) {
                    state.input_has_valid_match = true;
                }
                return Ok(());
            }
            crate::components::autocomplete::AutocompleteAction::Close => {
                return Ok(());
            }
            crate::components::autocomplete::AutocompleteAction::KeepOpen => {
                // Up/Down 导航或普通字符继续输入
                if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                    return Ok(());
                }
                // 其他按键 fall through 到主 input handler
            }
        }
    }

    // Jump 模式：等待字符输入
    if let Some(direction) = input.jump_mode {
        input.jump_mode = None;
        if let KeyCode::Char(target) = key.code {
            jump_to_char(input, target, direction);
        }
        return Ok(());
    }

    if matches_action(&keybindings, key, "app.suspend") {
        suspend_process();
        state.needs_full_redraw = true;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.model.cycleForward") {
        send(
            writer,
            &ClientMessage::CycleModel {
                direction: "forward",
            },
        )?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.model.cycleBackward") {
        send(
            writer,
            &ClientMessage::CycleModel {
                direction: "backward",
            },
        )?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.model.select") {
        send(writer, &ClientMessage::ListModels)?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.tools.expand")
        || (key.code == KeyCode::Char('o') && key.modifiers == KeyModifiers::CONTROL)
    {
        state.tools_expanded = !state.tools_expanded;
        state.compaction_collapsed = !state.compaction_collapsed;
        return Ok(());
    }
    // Alt+T — 切换 dark/light 主题
    if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::ALT {
        let new_theme = crate::theme::toggle_theme();
        send(
            writer,
            &ClientMessage::UpdateSetting {
                key: "theme",
                value: new_theme,
            },
        )?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.thinking.toggle") {
        state.thinking_visible = !state.thinking_visible;
        send(writer, &ClientMessage::CycleThinking)?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.editor.external") {
        if let Some(text) = open_external_editor(&input.text()) {
            input.set_text(text);
        }
        state.needs_full_redraw = true;
        return Ok(());
    }
    // subagent 切换：优先用 keybindings，同时硬编码 Ctrl+] / Ctrl+Shift+] 作为 fallback
    let is_subagent_next = matches_action(&keybindings, key, "app.subagent.next")
        || (key.code == KeyCode::Char(']') && key.modifiers.contains(KeyModifiers::CONTROL));
    if is_subagent_next {
        if let Some(id) = get_next_subagent(&state.ui, 1) {
            send(writer, &ClientMessage::SwitchAgent { id: &id })?;
        }
        return Ok(());
    }
    // Ctrl+[ 在终端中与 Escape 不可区分，使用 Alt+[ 作为 fallback
    let is_subagent_prev = matches_action(&keybindings, key, "app.subagent.previous")
        || (key.code == KeyCode::Char('[') && key.modifiers.contains(KeyModifiers::ALT));
    if is_subagent_prev {
        if let Some(id) = get_next_subagent(&state.ui, -1) {
            send(writer, &ClientMessage::SwitchAgent { id: &id })?;
        }
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.session.toggleNamedFilter") {
        send(writer, &ClientMessage::ListSessions { scope: "current" })?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "app.editMode.cycle") {
        send(writer, &ClientMessage::CycleEditMode)?;
        return Ok(());
    }
    if matches_action(&keybindings, key, "tui.editor.undo") {
        undo(input);
        return Ok(());
    }

    match key.code {
        // app.clear: ctrl+c — 清空编辑器；双击 500ms 内退出
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let now = std::time::Instant::now();
            if let Some(last) = state.last_ctrl_c {
                if now.duration_since(last) < std::time::Duration::from_millis(500) {
                    state.last_ctrl_c = None;
                    send(writer, &ClientMessage::Exit)?;
                    return Ok(());
                }
            }
            state.last_ctrl_c = Some(now);
            if input.is_empty() {
                if state.ui.is_streaming {
                    send(writer, &ClientMessage::Abort)?;
                }
            } else {
                input.clear();
            }
        }
        // app.exit: ctrl+d — 退出
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if input.is_empty() {
                send(writer, &ClientMessage::Exit)?;
            } else {
                delete_char_forward(input);
            }
        }
        // 以下绑定已通过 matches_action 统一处理，保留 fallback 以兼容无后端绑定情况
        // app.session.toggleNamedFilter: ctrl+n — 切换命名会话过滤
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            send(writer, &ClientMessage::ListSessions { scope: "current" })?;
        }
        // tui.editor.cursorLineStart: ctrl+a
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.cursor_col = 0;
        }
        // tui.editor.cursorLineEnd: ctrl+e
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.cursor_col = grapheme_count(&input.lines[input.cursor_row]);
        }
        // tui.editor.cursorLeft: ctrl+b
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if input.cursor_col > 0 {
                input.cursor_col -= 1;
            }
        }
        // tui.editor.cursorRight: ctrl+f
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let len = grapheme_count(&input.lines[input.cursor_row]);
            if input.cursor_col < len {
                input.cursor_col += 1;
            }
        }
        // tui.editor.deleteToLineStart: ctrl+u — kill to line start
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.push_undo();
            let line = &input.lines[input.cursor_row];
            let deleted = grapheme_take(line, input.cursor_col);
            if !deleted.is_empty() {
                let accumulate = input.last_action == Some(LastAction::Kill);
                input.kill_ring.push(
                    &deleted,
                    PushOpts {
                        prepend: true,
                        accumulate,
                    },
                );
            }
            let line = &mut input.lines[input.cursor_row];
            *line = grapheme_skip(line, input.cursor_col);
            input.cursor_col = 0;
            input.last_action = Some(LastAction::Kill);
        }
        // tui.editor.deleteToLineEnd: ctrl+k — kill to line end
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.push_undo();
            let line = &input.lines[input.cursor_row];
            let deleted = grapheme_skip(line, input.cursor_col);
            if !deleted.is_empty() {
                let accumulate = input.last_action == Some(LastAction::Kill);
                input.kill_ring.push(
                    &deleted,
                    PushOpts {
                        prepend: false,
                        accumulate,
                    },
                );
            }
            let line = &mut input.lines[input.cursor_row];
            *line = grapheme_take(line, input.cursor_col);
            input.last_action = Some(LastAction::Kill);
        }
        // tui.editor.deleteWordBackward: ctrl+w — kill word backward
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.push_undo();
            let deleted = delete_word_backward_text(input);
            if !deleted.is_empty() {
                let accumulate = input.last_action == Some(LastAction::Kill);
                input.kill_ring.push(
                    &deleted,
                    PushOpts {
                        prepend: true,
                        accumulate,
                    },
                );
            }
            input.last_action = Some(LastAction::Kill);
        }
        // tui.editor.yank: ctrl+y — yank (paste from kill ring)
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(text) = input.kill_ring.peek().map(|s| s.to_string()) {
                input.push_undo();
                let len = insert_text_at_cursor(input, &text);
                input.yank_len = len;
                input.last_action = Some(LastAction::Yank);
            }
        }
        // tui.editor.yankPop: alt+y — cycle kill ring
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::ALT) => {
            if input.last_action == Some(LastAction::Yank) && input.kill_ring.len() > 1 {
                // 删除刚 yank 的文本
                delete_chars_backward(input, input.yank_len);
                // 旋转 ring 并 yank 下一个
                input.kill_ring.rotate();
                if let Some(text) = input.kill_ring.peek().map(|s| s.to_string()) {
                    let len = insert_text_at_cursor(input, &text);
                    input.yank_len = len;
                }
                input.last_action = Some(LastAction::Yank);
            }
        }
        // tui.editor.jumpForward: alt+] — 进入跳转模式（前进）
        KeyCode::Char(']') if key.modifiers == KeyModifiers::ALT => {
            input.jump_mode = Some(JumpDirection::Forward);
        }
        // tui.editor.jumpBackward: alt+[ — 进入跳转模式（后退）
        KeyCode::Char('[') if key.modifiers == KeyModifiers::ALT => {
            input.jump_mode = Some(JumpDirection::Backward);
        }
        // tui.editor.foldAtCursor: alt+shift+[ — 折叠当前行所在块
        KeyCode::Char('{') if key.modifiers.contains(KeyModifiers::ALT) => {
            let row = input.cursor_row;
            let end = find_fold_end(input, row);
            if end > row {
                input.fold_range(row, end);
            }
        }
        // tui.editor.unfoldAtCursor: alt+shift+] — 展开当前行折叠
        KeyCode::Char('}') if key.modifiers.contains(KeyModifiers::ALT) => {
            input.unfold_at(input.cursor_row);
        }
        // tui.editor.wordForward: alt+f — 前进一个 word（标点感知）
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::ALT) => {
            move_word_forward(input);
        }
        // tui.editor.wordBackward: alt+b — 后退一个 word（标点感知）
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::ALT) => {
            move_word_backward(input);
        }
        // tui.editor.deleteWordForward: alt+d — kill word forward
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::ALT) => {
            input.push_undo();
            let deleted = delete_word_forward_text(input);
            if !deleted.is_empty() {
                let accumulate = input.last_action == Some(LastAction::Kill);
                input.kill_ring.push(
                    &deleted,
                    PushOpts {
                        prepend: false,
                        accumulate,
                    },
                );
            }
            input.last_action = Some(LastAction::Kill);
        }
        // app.editMode.cycle: shift+tab
        KeyCode::BackTab => {
            send(writer, &ClientMessage::CycleEditMode)?;
        }
        // Tab: 触发自动补全（仅在 /@ 上下文中）
        KeyCode::Tab => {
            let text = input.text();
            let cursor = cursor_char_index(input);
            if is_autocomplete_context(&text, cursor) {
                crate::components::autocomplete::request_autocomplete(&text, cursor, writer)?;
            }
        }
        KeyCode::Char(ch) => {
            // 有选区时先删除选区内容
            if input.selection_anchor.is_some() {
                input.delete_selection();
            }
            // Undo coalescing: 空白字符或非连续输入触发快照
            if ch.is_whitespace() || input.last_action != Some(LastAction::TypeWord) {
                input.push_undo();
            }
            input.last_action = Some(LastAction::TypeWord);
            insert_char(input, ch);
            let text = input.text();
            let cursor = cursor_char_index(input);
            if is_autocomplete_context(&text, cursor) {
                crate::components::autocomplete::request_autocomplete(&text, cursor, writer)?;
            } else {
                state.input_has_valid_match = false;
                state.autocomplete = None;
            }
        }
        KeyCode::Backspace => {
            if input.selection_anchor.is_some() {
                input.delete_selection();
            } else {
                input.push_undo();
                input.last_action = None;
                delete_char_backward(input);
            }
            let text = input.text();
            let cursor = cursor_char_index(input);
            if is_autocomplete_context(&text, cursor) {
                crate::components::autocomplete::request_autocomplete(&text, cursor, writer)?;
            } else {
                state.autocomplete = None;
                state.input_has_valid_match = false;
            }
        }
        KeyCode::Delete => {
            input.push_undo();
            input.last_action = None;
            delete_char_forward(input);
        }
        // Shift+Arrow: 文本选区
        KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if input.selection_anchor.is_none() {
                input.selection_anchor = Some(SelectionAnchor {
                    row: input.cursor_row,
                    col: input.cursor_col,
                });
            }
            if input.cursor_col > 0 {
                input.cursor_col -= 1;
            } else if input.cursor_row > 0 {
                input.cursor_row -= 1;
                input.cursor_col = grapheme_count(&input.lines[input.cursor_row]);
            }
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if input.selection_anchor.is_none() {
                input.selection_anchor = Some(SelectionAnchor {
                    row: input.cursor_row,
                    col: input.cursor_col,
                });
            }
            let len = grapheme_count(&input.lines[input.cursor_row]);
            if input.cursor_col < len {
                input.cursor_col += 1;
            } else if input.cursor_row + 1 < input.lines.len() {
                input.cursor_row += 1;
                input.cursor_col = 0;
            }
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if input.selection_anchor.is_none() {
                input.selection_anchor = Some(SelectionAnchor {
                    row: input.cursor_row,
                    col: input.cursor_col,
                });
            }
            if input.cursor_row > 0 {
                input.cursor_row -= 1;
                let len = grapheme_count(&input.lines[input.cursor_row]);
                input.cursor_col = input.cursor_col.min(len);
            }
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if input.selection_anchor.is_none() {
                input.selection_anchor = Some(SelectionAnchor {
                    row: input.cursor_row,
                    col: input.cursor_col,
                });
            }
            if input.cursor_row + 1 < input.lines.len() {
                input.cursor_row += 1;
                let len = grapheme_count(&input.lines[input.cursor_row]);
                input.cursor_col = input.cursor_col.min(len);
            }
        }
        KeyCode::Left => {
            input.last_action = None;
            input.clear_selection();
            if input.cursor_col > 0 {
                input.cursor_col -= 1;
                if let Some(span) = find_atomic_span(input) {
                    input.cursor_col = span.col_start;
                }
            } else if input.cursor_row > 0 {
                input.cursor_row -= 1;
                input.cursor_col = grapheme_count(&input.lines[input.cursor_row]);
            }
        }
        KeyCode::Right => {
            input.last_action = None;
            input.clear_selection();
            let len = grapheme_count(&input.lines[input.cursor_row]);
            if input.cursor_col < len {
                input.cursor_col += 1;
                if let Some(span) = find_atomic_span(input) {
                    input.cursor_col = span.col_start + span.col_len;
                }
            } else if input.cursor_row + 1 < input.lines.len() {
                input.cursor_row += 1;
                input.cursor_col = 0;
            }
        }
        KeyCode::Home => {
            input.last_action = None;
            input.cursor_col = 0;
        }
        KeyCode::End => {
            input.last_action = None;
            input.cursor_col = grapheme_count(&input.lines[input.cursor_row]);
        }
        KeyCode::PageUp => {
            state.scroll = state.scroll.saturating_add(10);
            state.auto_scroll = false;
        }
        KeyCode::PageDown => {
            state.scroll = state.scroll.saturating_sub(10);
            if state.scroll == 0 {
                state.auto_scroll = true;
            }
        }
        KeyCode::Up => {
            if input.cursor_row > 0 {
                input.cursor_row -= 1;
                let len = grapheme_count(&input.lines[input.cursor_row]);
                input.cursor_col = input.cursor_col.min(len);
            } else if input.is_empty() && !input.history.is_empty() {
                let idx = match input.history_index {
                    Some(i) => i.saturating_sub(1),
                    None => input.history.len() - 1,
                };
                input.history_index = Some(idx);
                input.set_text(input.history[idx].clone());
            } else if input.is_empty() {
                state.scroll = state.scroll.saturating_add(1);
                state.auto_scroll = false;
            }
        }
        KeyCode::Down => {
            if input.cursor_row + 1 < input.lines.len() {
                input.cursor_row += 1;
                let len = grapheme_count(&input.lines[input.cursor_row]);
                input.cursor_col = input.cursor_col.min(len);
            } else if let Some(idx) = input.history_index {
                if idx + 1 < input.history.len() {
                    input.history_index = Some(idx + 1);
                    input.set_text(input.history[idx + 1].clone());
                } else {
                    input.history_index = None;
                    input.clear();
                }
            } else if input.is_empty() {
                state.scroll = state.scroll.saturating_sub(1);
                if state.scroll == 0 {
                    state.auto_scroll = true;
                }
            }
        }
        // app.message.followUp: alt+enter — 队列 follow-up 消息
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            let text = input.expanded_text().trim().to_string();
            if text.is_empty() {
                return Ok(());
            }
            input.history.push(text.clone());
            input.clear();
            input.history_index = None;
            let images = take_images(&mut state.attached_images);
            send(
                writer,
                &ClientMessage::FollowUp {
                    text: &text,
                    images,
                },
            )?;
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            input.push_undo();
            input.last_action = None;
            let line = &mut input.lines[input.cursor_row];
            let tail = grapheme_skip(line, input.cursor_col);
            *line = grapheme_take(line, input.cursor_col);
            input.cursor_row += 1;
            input.lines.insert(input.cursor_row, tail);
            input.cursor_col = 0;
        }
        KeyCode::Enter => {
            let text = input.expanded_text().trim().to_string();
            if text.is_empty() {
                return Ok(());
            }
            input.history.push(text.clone());
            input.clear();
            input.history_index = None;
            state.input_has_valid_match = false;
            if let Some(cmd) = text.strip_prefix('!') {
                let cmd = cmd.trim();
                send(writer, &ClientMessage::Bash { command: cmd })?;
            } else if crate::command::is_local_command(&text) {
                // 本地处理 /theme — 切换或设置主题
                let new_theme = if text == "/theme dark" {
                    crate::theme::set_theme(crate::theme::Theme::dark());
                    "dark"
                } else if text == "/theme light" {
                    crate::theme::set_theme(crate::theme::Theme::light());
                    "light"
                } else {
                    crate::theme::toggle_theme()
                };
                send(
                    writer,
                    &ClientMessage::UpdateSetting {
                        key: "theme",
                        value: new_theme,
                    },
                )?;
                state.needs_full_redraw = true;
            } else if text.starts_with('/') {
                // slash command 始终走 submit，后端 handleNativeBuiltinCommand 统一路由
                let images = take_images(&mut state.attached_images);
                send(
                    writer,
                    &ClientMessage::Submit {
                        text: &text,
                        images,
                    },
                )?;
            } else if state.ui.is_streaming {
                let images = take_images(&mut state.attached_images);
                send(
                    writer,
                    &ClientMessage::FollowUp {
                        text: &text,
                        images,
                    },
                )?;
            } else {
                let images = take_images(&mut state.attached_images);
                send(
                    writer,
                    &ClientMessage::Submit {
                        text: &text,
                        images,
                    },
                )?;
            }
        }
        // app.interrupt: escape — streaming 时中断；空编辑器双击 Esc 打开 graph
        KeyCode::Esc => {
            if state.ui.is_streaming {
                send(writer, &ClientMessage::Abort)?;
            } else if input.is_empty() {
                let now = std::time::Instant::now();
                if let Some(last) = state.last_escape {
                    if now.duration_since(last) < std::time::Duration::from_millis(500) {
                        state.last_escape = None;
                        send(
                            writer,
                            &ClientMessage::Submit {
                                text: "/graph",
                                images: Vec::new(),
                            },
                        )?;
                    } else {
                        state.last_escape = Some(now);
                    }
                } else {
                    state.last_escape = Some(now);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn take_images(attached: &mut Vec<String>) -> Vec<ImagePayload> {
    std::mem::take(attached)
        .into_iter()
        .map(ImagePayload::from_base64)
        .collect()
}

fn handle_dialog_key(
    key: KeyEvent,
    state: &mut AppState,
    writer: &Writer,
    mut dialog: DialogState,
    keybindings: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), Box<dyn Error>> {
    let is_cancel = matches_action(keybindings, key, "tui.select.cancel")
        || matches!(key.code, KeyCode::Esc);
    let is_up = matches_action(keybindings, key, "tui.select.up")
        || matches!(key.code, KeyCode::Up)
        || (key.code == KeyCode::Char('k') && key.modifiers == KeyModifiers::NONE && dialog.kind != "input" && dialog.kind != "editor")
        || (key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL));
    let is_down = matches_action(keybindings, key, "tui.select.down")
        || matches!(key.code, KeyCode::Down)
        || (key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::NONE && dialog.kind != "input" && dialog.kind != "editor")
        || (key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL));
    let is_confirm = matches_action(keybindings, key, "tui.select.confirm")
        || matches!(key.code, KeyCode::Enter);

    if is_cancel {
        send(
            writer,
            &ClientMessage::DialogResponse {
                id: &dialog.id,
                value: None,
                confirmed: None,
                cancelled: Some(true),
            },
        )?;
        state.dialog = None;
    } else if is_up {
        if dialog.selected == 0 {
            dialog.selected = dialog.options.len().saturating_sub(1);
        } else {
            dialog.selected -= 1;
        }
        state.dialog = Some(dialog);
    } else if is_down {
        if dialog.selected + 1 >= dialog.options.len() {
            dialog.selected = 0;
        } else {
            dialog.selected += 1;
        }
        state.dialog = Some(dialog);
    } else if matches_action(keybindings, key, "tui.editor.deleteCharBackward")
        || matches!(key.code, KeyCode::Backspace)
    {
        if dialog.kind == "input" || dialog.kind == "editor" {
            dialog.input.pop();
            state.dialog = Some(dialog);
        }
    } else if is_confirm {
        // 拦截本地注入的 theme 选项
        let selected_value = dialog.options.get(dialog.selected).map(String::as_str);
        if let Some(val) = selected_value {
            if val.starts_with("Theme:") {
                let new_theme = crate::theme::toggle_theme();
                send(
                    writer,
                    &ClientMessage::UpdateSetting {
                        key: "theme",
                        value: new_theme,
                    },
                )?;
                state.dialog = None;
                state.needs_full_redraw = true;
                return Ok(());
            }
        }
        let input_value = dialog.input.as_str();
        let response = match dialog.kind.as_str() {
            "confirm" => ClientMessage::DialogResponse {
                id: &dialog.id,
                value: None,
                confirmed: Some(dialog.selected == 0),
                cancelled: None,
            },
            "select" => ClientMessage::DialogResponse {
                id: &dialog.id,
                value: selected_value,
                confirmed: None,
                cancelled: None,
            },
            _ => ClientMessage::DialogResponse {
                id: &dialog.id,
                value: Some(input_value),
                confirmed: None,
                cancelled: None,
            },
        };
        send(writer, &response)?;
        state.dialog = None;
    } else if let KeyCode::Char(ch) = key.code {
        if dialog.kind == "input" || dialog.kind == "editor" {
            dialog.input.push(ch);
            state.dialog = Some(dialog);
        }
    }
    Ok(())
}

/// 暂停 TUI，打开外部编辑器编辑文本，返回编辑后内容
fn open_external_editor(current_text: &str) -> Option<String> {
    let editor = env::var("EDITOR")
        .or_else(|_| env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let tmp_path = env::temp_dir().join(format!("pi_input_{}.txt", std::process::id()));

    {
        let mut file = fs::File::create(&tmp_path).ok()?;
        file.write_all(current_text.as_bytes()).ok()?;
    }

    // 暂停 TUI
    crossterm::terminal::disable_raw_mode().ok()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        crossterm::terminal::LeaveAlternateScreen
    ).ok()?;

    let status = Command::new(&editor).arg(&tmp_path).status().ok()?;

    // 恢复 TUI — 完全恢复所有终端模式
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableMouseCapture
    ).ok()?;
    crossterm::terminal::enable_raw_mode().ok()?;

    if !status.success() {
        let _ = fs::remove_file(&tmp_path);
        return None;
    }

    let content = fs::read_to_string(&tmp_path).ok()?;
    let _ = fs::remove_file(&tmp_path);
    Some(content.trim_end_matches('\n').to_string())
}

/// 发送 SIGTSTP 挂起进程（恢复时重新进入 raw mode + alternate screen）
fn suspend_process() {
    crossterm::terminal::disable_raw_mode().ok();
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        crossterm::terminal::LeaveAlternateScreen
    ).ok();

    #[cfg(unix)]
    unsafe {
        libc::raise(libc::SIGTSTP);
    }

    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableMouseCapture
    ).ok();
    crossterm::terminal::enable_raw_mode().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_insert() {
        let mut input = InputState::default();
        insert_char(&mut input, 'h');
        insert_char(&mut input, 'i');
        assert_eq!(input.text(), "hi");
        assert_eq!(input.cursor_col, 2);
    }

    #[test]
    fn word_delete() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_col = 11;
        let deleted = delete_word_backward_text(&mut input);
        assert_eq!(deleted, "world");
        assert_eq!(input.text(), "hello ");
    }

    #[test]
    fn undo_basic() {
        let mut input = InputState::default();
        input.push_undo();
        insert_char(&mut input, 'a');
        insert_char(&mut input, 'b');
        undo(&mut input);
        assert_eq!(input.text(), "");
    }

    #[test]
    fn kill_ring_ctrl_k_ctrl_y() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_col = 5;
        // Ctrl+K: kill to end
        input.push_undo();
        let line = &input.lines[input.cursor_row];
        let deleted: String = line.chars().skip(input.cursor_col).collect();
        input.kill_ring.push(
            &deleted,
            PushOpts {
                prepend: false,
                accumulate: false,
            },
        );
        let line = &mut input.lines[input.cursor_row];
        *line = line.chars().take(input.cursor_col).collect();
        assert_eq!(input.text(), "hello");
        assert_eq!(input.kill_ring.peek(), Some(" world"));

        // Ctrl+Y: yank
        let text = input.kill_ring.peek().unwrap().to_string();
        insert_text_at_cursor(&mut input, &text);
        assert_eq!(input.text(), "hello world");
    }

    #[test]
    fn kill_ring_accumulate() {
        let mut input = InputState::default();
        input.kill_ring.push(
            "first",
            PushOpts {
                prepend: false,
                accumulate: false,
            },
        );
        input.kill_ring.push(
            " second",
            PushOpts {
                prepend: false,
                accumulate: true,
            },
        );
        assert_eq!(input.kill_ring.peek(), Some("first second"));
    }

    #[test]
    fn delete_word_forward() {
        let mut input = InputState::default();
        input.set_text("hello world end".to_string());
        input.cursor_col = 6;
        let deleted = delete_word_forward_text(&mut input);
        assert_eq!(deleted, "world ");
        assert_eq!(input.text(), "hello end");
    }

    #[test]
    fn autocomplete_context_accepts_at_token_after_text() {
        let text = "please read @src/ma";
        assert!(is_autocomplete_context(text, text.chars().count()));
        assert!(!is_autocomplete_context("email foo@example.com", 6));
    }

    #[test]
    fn cursor_char_index_counts_previous_lines() {
        let mut input = InputState::default();
        input.set_text("first\nsecond".to_string());
        input.cursor_row = 1;
        input.cursor_col = 3;
        assert_eq!(cursor_char_index(&input), 9);
    }

    #[test]
    fn jump_forward_to_char() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_col = 0;
        jump_to_char(&mut input, 'o', JumpDirection::Forward);
        assert_eq!(input.cursor_col, 4); // 'o' in "hello"
        jump_to_char(&mut input, 'o', JumpDirection::Forward);
        assert_eq!(input.cursor_col, 7); // 'o' in "world"
    }

    #[test]
    fn jump_backward_to_char() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_col = 10;
        jump_to_char(&mut input, 'l', JumpDirection::Backward);
        assert_eq!(input.cursor_col, 9); // 'l' in "world"
        jump_to_char(&mut input, 'l', JumpDirection::Backward);
        assert_eq!(input.cursor_col, 3); // second 'l' in "hello"
    }

    // --- Grapheme-aware editing tests ---

    #[test]
    fn grapheme_insert_emoji() {
        let mut input = InputState::default();
        input.set_text("ab".to_string());
        input.cursor_col = 1;
        insert_char(&mut input, '\u{1F389}');
        assert_eq!(input.text(), "a\u{1F389}b");
        assert_eq!(input.cursor_col, 2);
    }

    #[test]
    fn grapheme_delete_backward_emoji() {
        let mut input = InputState::default();
        input.set_text("a\u{1F389}b".to_string());
        input.cursor_col = 2;
        delete_char_backward(&mut input);
        assert_eq!(input.text(), "ab");
        assert_eq!(input.cursor_col, 1);
    }

    #[test]
    fn grapheme_delete_forward_emoji() {
        let mut input = InputState::default();
        input.set_text("a\u{1F389}b".to_string());
        input.cursor_col = 1;
        delete_char_forward(&mut input);
        assert_eq!(input.text(), "ab");
        assert_eq!(input.cursor_col, 1);
    }

    #[test]
    fn grapheme_count_multibyte() {
        assert_eq!(grapheme_count("hello"), 5);
        assert_eq!(grapheme_count("\u{4F60}\u{597D}\u{4E16}\u{754C}"), 4);
        assert_eq!(grapheme_count("a\u{1F1EF}\u{1F1F5}b"), 3); // flag emoji = 1 grapheme
    }

    #[test]
    fn grapheme_take_and_skip() {
        let s = "a\u{1F389}b\u{1F680}c";
        assert_eq!(grapheme_take(s, 2), "a\u{1F389}");
        assert_eq!(grapheme_skip(s, 2), "b\u{1F680}c");
        assert_eq!(grapheme_take(s, 0), "");
        assert_eq!(grapheme_skip(s, 5), "");
    }

    #[test]
    fn grapheme_to_byte_offset_chinese() {
        let s = "\u{4F60}\u{597D}\u{4E16}\u{754C}";
        assert_eq!(grapheme_to_byte_offset(s, 0), 0);
        assert_eq!(grapheme_to_byte_offset(s, 1), 3); // 一个中文字 3 bytes
        assert_eq!(grapheme_to_byte_offset(s, 4), s.len());
    }

    #[test]
    fn is_word_char_classification() {
        assert!(is_word_char("a"));
        assert!(is_word_char("Z"));
        assert!(is_word_char("_"));
        assert!(is_word_char("5"));
        assert!(!is_word_char("."));
        assert!(!is_word_char(" "));
        assert!(!is_word_char("!"));
    }

    // --- Word movement with punctuation awareness ---

    #[test]
    fn word_delete_backward_punctuation() {
        let mut input = InputState::default();
        input.set_text("foo.bar baz".to_string());
        input.cursor_col = 7; // after "foo.bar"
        let deleted = delete_word_backward_text(&mut input);
        assert_eq!(deleted, "bar");
        assert_eq!(input.text(), "foo. baz");
    }

    #[test]
    fn word_delete_forward_punctuation() {
        let mut input = InputState::default();
        input.set_text("foo.bar baz".to_string());
        input.cursor_col = 0;
        let deleted = delete_word_forward_text(&mut input);
        assert_eq!(deleted, "foo");
        assert_eq!(input.text(), ".bar baz");
    }

    // --- Text selection tests ---

    #[test]
    fn selection_single_line() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_row = 0;
        input.cursor_col = 6;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 0 });
        assert_eq!(input.selection_range(), Some((0, 0, 0, 6)));
        assert_eq!(input.selected_text(), Some("hello ".to_string()));
    }

    #[test]
    fn selection_multi_line() {
        let mut input = InputState::default();
        input.set_text("first\nsecond\nthird".to_string());
        input.cursor_row = 2;
        input.cursor_col = 3;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
        let text = input.selected_text().unwrap();
        assert_eq!(text, "st\nsecond\nthi");
    }

    #[test]
    fn selection_reversed_anchor() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_row = 0;
        input.cursor_col = 2;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 8 });
        assert_eq!(input.selection_range(), Some((0, 2, 0, 8)));
        assert_eq!(input.selected_text(), Some("llo wo".to_string()));
    }

    #[test]
    fn selection_empty_returns_none() {
        let mut input = InputState::default();
        input.set_text("hello".to_string());
        input.cursor_row = 0;
        input.cursor_col = 3;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
        assert_eq!(input.selection_range(), None);
    }

    #[test]
    fn delete_selection_single_line() {
        let mut input = InputState::default();
        input.set_text("hello world".to_string());
        input.cursor_row = 0;
        input.cursor_col = 5;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 0 });
        let deleted = input.delete_selection().unwrap();
        assert_eq!(deleted, "hello");
        assert_eq!(input.text(), " world");
        assert_eq!(input.cursor_col, 0);
        assert!(input.selection_anchor.is_none());
    }

    #[test]
    fn delete_selection_multi_line() {
        let mut input = InputState::default();
        input.set_text("first\nsecond\nthird".to_string());
        input.cursor_row = 2;
        input.cursor_col = 2;
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 3 });
        let deleted = input.delete_selection().unwrap();
        assert_eq!(deleted, "st\nsecond\nth");
        assert_eq!(input.text(), "firird");
        assert_eq!(input.cursor_row, 0);
        assert_eq!(input.cursor_col, 3);
    }

    #[test]
    fn clear_selection() {
        let mut input = InputState::default();
        input.selection_anchor = Some(SelectionAnchor { row: 0, col: 5 });
        input.clear_selection();
        assert!(input.selection_anchor.is_none());
    }

    // --- Multiline delete_char_backward joining lines ---

    #[test]
    fn delete_backward_joins_lines() {
        let mut input = InputState::default();
        input.set_text("first\nsecond".to_string());
        input.cursor_row = 1;
        input.cursor_col = 0;
        delete_char_backward(&mut input);
        assert_eq!(input.text(), "firstsecond");
        assert_eq!(input.cursor_row, 0);
        assert_eq!(input.cursor_col, 5);
    }

    // --- delete_char_forward joining lines ---

    #[test]
    fn delete_forward_joins_lines() {
        let mut input = InputState::default();
        input.set_text("first\nsecond".to_string());
        input.cursor_row = 0;
        input.cursor_col = 5; // at end of "first"
        delete_char_forward(&mut input);
        assert_eq!(input.text(), "firstsecond");
    }
}
