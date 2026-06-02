// input/mouse.rs — 鼠标事件 + 粘贴处理
//
// 内部结构:
// mouse.rs
// ├── handle_mouse()     # 鼠标滚轮事件处理
// ├── handle_paste()     # 粘贴事件处理（文本 / 图片 / 大段折叠）
// ├── is_image_path()    # 判断文件路径是否为图片
// └── attach_image()     # 附加图片到编辑器
//
// 相关文档:
// - [SPEC](../../../../.dev-doc/refactor/tui/SPEC.md)

use crossterm::event::MouseEvent;
use unicode_segmentation::UnicodeSegmentation;

use crate::app::AppState;
use crate::input::{AtomicSpan, InputState};

/// 处理鼠标滚轮事件
pub fn handle_mouse(mouse: MouseEvent, state: &mut AppState) {
    use crossterm::event::MouseEventKind;
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            state.scroll = state.scroll.saturating_add(1);
            state.auto_scroll = false;
        }
        MouseEventKind::ScrollDown => {
            state.scroll = state.scroll.saturating_sub(1);
            if state.scroll == 0 {
                state.auto_scroll = true;
            }
        }
        _ => {}
    }
}

/// 判断文件路径是否为图片
pub fn is_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
}

/// 附加图片到编辑器（插入 [image #N] marker 并注册原子 span）
pub fn attach_image(state: &mut AppState, editor: &mut InputState, image_data: String) {
    state.attached_images.push(image_data);
    let img_id = state.attached_images.len();
    let marker = format!("[image #{img_id}]");
    let col_start = editor.cursor_col;
    let marker_len = marker.graphemes(true).count();
    let line = &mut editor.lines[editor.cursor_row];
    let byte_offset = line
        .grapheme_indices(true)
        .nth(editor.cursor_col)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    line.insert_str(byte_offset, &marker);
    editor.atomic_spans.push(AtomicSpan {
        row: editor.cursor_row,
        col_start,
        col_len: marker_len,
    });
    editor.cursor_col += marker_len;
}

/// 处理粘贴事件
pub fn handle_paste(data: &str, state: &mut AppState, editor: &mut InputState) {
    let trimmed = data.trim();

    // base64 图片数据
    let is_base64_image = trimmed.starts_with("iVBOR")
        || trimmed.starts_with("/9j/")
        || trimmed.starts_with("R0lGOD")
        || trimmed.starts_with("UklGR");

    if is_base64_image
        && base64::Engine::decode(&base64::engine::general_purpose::STANDARD, trimmed).is_ok()
    {
        attach_image(state, editor, trimmed.to_string());
        return;
    }

    // 图片文件路径（单行，文件存在且有图片扩展名）
    // 去除可能的引号包裹
    if !trimmed.contains('\n') {
        let stripped = trimmed.trim_matches(|c| c == '\'' || c == '"');
        if is_image_path(stripped) {
            let path = std::path::Path::new(stripped);
            if path.is_file() {
                if let Ok(bytes) = std::fs::read(path) {
                    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                    attach_image(state, editor, b64);
                    return;
                }
            }
        }
    }

    {
        let normalized = crate::normalize_newlines(data);
        let pasted_lines: Vec<&str> = normalized.lines().collect();
        let total_chars = normalized.len();

        // 大段粘贴折叠：>10 行或 >1000 字符时插入 marker
        if pasted_lines.len() > 10 || total_chars > 1000 {
            editor.push_undo();
            editor.last_action = None;
            editor.paste_counter += 1;
            let paste_id = editor.paste_counter;
            editor.pastes.push(normalized.clone());
            let marker = if pasted_lines.len() > 10 {
                format!("[paste #{paste_id} +{} lines]", pasted_lines.len())
            } else {
                format!("[paste #{paste_id} {total_chars} chars]")
            };
            let col_start = editor.cursor_col;
            let marker_len = marker.graphemes(true).count();
            let line = &mut editor.lines[editor.cursor_row];
            let byte_offset = line
                .grapheme_indices(true)
                .nth(editor.cursor_col)
                .map(|(i, _)| i)
                .unwrap_or(line.len());
            line.insert_str(byte_offset, &marker);
            // 注册原子 span
            editor.atomic_spans.push(AtomicSpan {
                row: editor.cursor_row,
                col_start,
                col_len: marker_len,
            });
            editor.cursor_col += marker_len;
            return;
        }

        // 短文本直接插入
        editor.push_undo();
        editor.last_action = None;
        for g in normalized.graphemes(true) {
            if g == "\n" {
                let line = &mut editor.lines[editor.cursor_row];
                let byte_offset = line
                    .grapheme_indices(true)
                    .nth(editor.cursor_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                let tail = line[byte_offset..].to_string();
                line.truncate(byte_offset);
                editor.cursor_row += 1;
                editor.lines.insert(editor.cursor_row, tail);
                editor.cursor_col = 0;
            } else {
                let line = &mut editor.lines[editor.cursor_row];
                let byte_offset = line
                    .grapheme_indices(true)
                    .nth(editor.cursor_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                line.insert_str(byte_offset, g);
                editor.cursor_col += 1;
            }
        }
    }
}
