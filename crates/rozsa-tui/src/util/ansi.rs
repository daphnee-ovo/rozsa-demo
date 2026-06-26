use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn parse_ansi_line(text: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style = Style::default();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '\x1b' && i + 1 < len && chars[i + 1] == '[' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), style));
                buf.clear();
            }
            i += 2;
            let mut params = String::new();
            while i < len && chars[i] != 'm' {
                params.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1;
            }
            style = apply_sgr(&params, style);
        } else {
            buf.push(chars[i]);
            i += 1;
        }
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, style));
    }

    Line::from(spans)
}

fn apply_sgr(params: &str, mut style: Style) -> Style {
    let codes: Vec<&str> = params.split(';').collect();
    let mut i = 0;
    while i < codes.len() {
        let n: u16 = codes[i].parse().unwrap_or(0);
        match n {
            0 => style = Style::default(),
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            30 => style = style.fg(Color::Black),
            31 => style = style.fg(Color::Red),
            32 => style = style.fg(Color::Green),
            33 => style = style.fg(Color::Yellow),
            34 => style = style.fg(Color::Blue),
            35 => style = style.fg(Color::Magenta),
            36 => style = style.fg(Color::Cyan),
            37 => style = style.fg(Color::White),
            38 => {
                if let Some(color) = parse_extended_color(&codes, &mut i) {
                    style = style.fg(color);
                }
            }
            39 => style = style.fg(Color::Reset),
            40 => style = style.bg(Color::Black),
            41 => style = style.bg(Color::Red),
            42 => style = style.bg(Color::Green),
            43 => style = style.bg(Color::Yellow),
            44 => style = style.bg(Color::Blue),
            45 => style = style.bg(Color::Magenta),
            46 => style = style.bg(Color::Cyan),
            47 => style = style.bg(Color::White),
            48 => {
                if let Some(color) = parse_extended_color(&codes, &mut i) {
                    style = style.bg(color);
                }
            }
            49 => style = style.bg(Color::Reset),
            90 => style = style.fg(Color::DarkGray),
            91 => style = style.fg(Color::LightRed),
            92 => style = style.fg(Color::LightGreen),
            93 => style = style.fg(Color::LightYellow),
            94 => style = style.fg(Color::LightBlue),
            95 => style = style.fg(Color::LightMagenta),
            96 => style = style.fg(Color::LightCyan),
            97 => style = style.fg(Color::White),
            _ => {}
        }
        i += 1;
    }
    style
}

/// 解析 256 色 (38;5;N) 或 RGB 真彩色 (38;2;R;G;B)
fn parse_extended_color(codes: &[&str], i: &mut usize) -> Option<Color> {
    if *i + 1 >= codes.len() {
        return None;
    }
    let mode: u8 = codes[*i + 1].parse().ok()?;
    match mode {
        5 => {
            // 256 色: 38;5;N
            if *i + 2 >= codes.len() {
                return None;
            }
            let n: u8 = codes[*i + 2].parse().ok()?;
            *i += 2;
            Some(Color::Indexed(n))
        }
        2 => {
            // RGB: 38;2;R;G;B
            if *i + 4 >= codes.len() {
                return None;
            }
            let r: u8 = codes[*i + 2].parse().ok()?;
            let g: u8 = codes[*i + 3].parse().ok()?;
            let b: u8 = codes[*i + 4].parse().ok()?;
            *i += 4;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}
