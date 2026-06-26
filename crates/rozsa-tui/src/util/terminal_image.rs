// terminal_image.rs — 终端图片渲染（Kitty + iTerm2 协议）
//
// Internal Framework:
// terminal_image.rs
// ├── ImageProtocol          — 协议类型
// ├── ImageDimensions        — 图片尺寸
// ├── parse_image_dimensions() — 从二进制头解析 PNG/JPEG/GIF/WebP 尺寸
// ├── kitty_transmit()       — Kitty 图片协议传输
// ├── iterm2_transmit()      — iTerm2 图片协议传输
// └── render_image()         — 根据终端能力选择协议渲染
//
// Related Docs:
// - [Kitty Graphics Protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
// - [iTerm2 Inline Images](https://iterm2.com/documentation-images.html)
// - [codex-rs image_protocol](../../../codex/codex-rs/tui/src/pets/image_protocol.rs)

use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;

use crate::util::terminal_caps::{ImageProtocol, CAPS};

const KITTY_CHUNK_SIZE: usize = 4096;

static IMAGE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// 从二进制数据解析图片尺寸
pub fn parse_image_dimensions(data: &[u8]) -> Option<ImageDimensions> {
    if data.len() < 8 {
        return None;
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A ... IHDR 在 offset 16
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() >= 24 {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some(ImageDimensions { width, height });
    }

    // JPEG: FF D8 FF ... 扫描 SOF 标记
    if data.starts_with(b"\xFF\xD8\xFF") {
        return parse_jpeg_dimensions(data);
    }

    // GIF: GIF87a 或 GIF89a
    if (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) && data.len() >= 10 {
        let width = u16::from_le_bytes([data[6], data[7]]) as u32;
        let height = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some(ImageDimensions { width, height });
    }

    // WebP: RIFF....WEBP
    if data.len() >= 30 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return parse_webp_dimensions(data);
    }

    None
}

fn parse_jpeg_dimensions(data: &[u8]) -> Option<ImageDimensions> {
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;

        // SOF0-SOF2 标记包含尺寸信息
        if (0xC0..=0xC2).contains(&marker) {
            if i + 7 > data.len() {
                return None;
            }
            let height = u16::from_be_bytes([data[i + 3], data[i + 4]]) as u32;
            let width = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            return Some(ImageDimensions { width, height });
        }

        // 跳过非 SOF 段
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        if i + 2 > data.len() {
            return None;
        }
        let seg_len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
        i += seg_len;
    }
    None
}

fn parse_webp_dimensions(data: &[u8]) -> Option<ImageDimensions> {
    // VP8 (lossy)
    if data.len() >= 30 && &data[12..16] == b"VP8 " {
        if data.len() < 30 {
            return None;
        }
        let width = (u16::from_le_bytes([data[26], data[27]]) & 0x3FFF) as u32;
        let height = (u16::from_le_bytes([data[28], data[29]]) & 0x3FFF) as u32;
        return Some(ImageDimensions { width, height });
    }

    // VP8L (lossless)
    if data.len() >= 25 && &data[12..16] == b"VP8L" {
        let b0 = data[21] as u32;
        let b1 = data[22] as u32;
        let b2 = data[23] as u32;
        let b3 = data[24] as u32;
        let bits = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
        let width = (bits & 0x3FFF) + 1;
        let height = ((bits >> 14) & 0x3FFF) + 1;
        return Some(ImageDimensions { width, height });
    }

    None
}

/// 查询终端 cell 像素尺寸（CSI 16t），失败时回退到默认值
fn query_cell_pixel_size() -> (u32, u32) {
    use std::sync::OnceLock;
    static CELL_SIZE: OnceLock<(u32, u32)> = OnceLock::new();
    *CELL_SIZE.get_or_init(|| {
        // 尝试通过 ioctl TIOCGWINSZ 获取像素尺寸
        #[cfg(unix)]
        {
            use std::mem::MaybeUninit;
            let mut ws = MaybeUninit::<libc::winsize>::zeroed();
            let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
            if ret == 0 {
                let ws = unsafe { ws.assume_init() };
                if ws.ws_xpixel > 0 && ws.ws_ypixel > 0 && ws.ws_col > 0 && ws.ws_row > 0 {
                    let cw = ws.ws_xpixel as u32 / ws.ws_col as u32;
                    let ch = ws.ws_ypixel as u32 / ws.ws_row as u32;
                    if cw > 0 && ch > 0 {
                        return (cw, ch);
                    }
                }
            }
        }
        // 回退到标准终端字体比例
        (8, 16)
    })
}

/// 计算图片在终端中应占的行列数
pub fn calculate_cell_size(img: ImageDimensions, max_cols: u16, max_rows: u16) -> (u16, u16) {
    let (cell_width, cell_height) = query_cell_pixel_size();

    let img_cols = (img.width + cell_width - 1) / cell_width;
    let img_rows = (img.height + cell_height - 1) / cell_height;

    let scale_x = max_cols as f64 / img_cols.max(1) as f64;
    let scale_y = max_rows as f64 / img_rows.max(1) as f64;
    let scale = scale_x.min(scale_y).min(1.0);

    let cols = ((img_cols as f64 * scale).ceil() as u16).max(1).min(max_cols);
    let rows = ((img_rows as f64 * scale).ceil() as u16).max(1).min(max_rows);

    (cols, rows)
}

/// 使用 Kitty 图形协议传输图片
pub fn kitty_transmit(data: &[u8], cols: u16, rows: u16) -> String {
    let image_id = IMAGE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let payload = base64::engine::general_purpose::STANDARD.encode(data);
    let chunks: Vec<&[u8]> = payload.as_bytes().chunks(KITTY_CHUNK_SIZE).collect();

    let mut command = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        let has_more = index + 1 < chunks.len();
        let more_flag = u8::from(has_more);
        if index == 0 {
            command.push_str(&format!(
                "\x1b_Ga=T,t=d,f=100,c={cols},r={rows},i={image_id},q=2,m={more_flag};{chunk_str}\x1b\\"
            ));
        } else {
            command.push_str(&format!("\x1b_Gm={more_flag};{chunk_str}\x1b\\"));
        }
    }
    command
}

/// 删除 Kitty 图片
pub fn kitty_delete(image_id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={image_id},q=2;\x1b\\")
}

/// 使用 iTerm2 协议传输图片
pub fn iterm2_transmit(data: &[u8], cols: u16) -> String {
    let payload = base64::engine::general_purpose::STANDARD.encode(data);
    format!(
        "\x1b]1337;File=inline=1;width={cols};preserveAspectRatio=1:{payload}\x07"
    )
}

/// 根据终端能力渲染图片，返回转义序列 + 占用的行数
pub fn render_image(data: &[u8], max_cols: u16, max_rows: u16) -> Option<(String, u16)> {
    let protocol = CAPS.images?;
    let dims = parse_image_dimensions(data)?;
    let (cols, rows) = calculate_cell_size(dims, max_cols, max_rows);

    let sequence = match protocol {
        ImageProtocol::Kitty => kitty_transmit(data, cols, rows),
        ImageProtocol::Iterm2 => iterm2_transmit(data, cols),
    };

    Some((sequence, rows))
}
