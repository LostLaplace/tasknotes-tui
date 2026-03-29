use anyhow::Result;
use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier, Style},
    Terminal,
};

use crate::{app::App, ui};

pub fn render_text(app: &App, width: u16, height: u16) -> Result<String> {
    let buffer = render_buffer(app, width, height)?;
    Ok(buffer_to_string(&buffer))
}

pub fn render_html(app: &App, width: u16, height: u16) -> Result<String> {
    let buffer = render_buffer(app, width, height)?;
    Ok(buffer_to_html(&buffer))
}

pub fn render_svg(app: &App, width: u16, height: u16) -> Result<String> {
    let buffer = render_buffer(app, width, height)?;
    Ok(buffer_to_svg(&buffer))
}

fn render_buffer(app: &App, width: u16, height: u16) -> Result<Buffer> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, app))?;
    Ok(terminal.backend().buffer().clone())
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let width = buffer.area.width as usize;
    let height = buffer.area.height as usize;
    let mut out = String::new();
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            let cell = &buffer.content[y * width + x];
            let symbol = cell.symbol();
            if symbol.is_empty() {
                line.push(' ');
            } else {
                line.push_str(symbol);
            }
        }
        while line.ends_with(' ') {
            line.pop();
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn buffer_to_html(buffer: &Buffer) -> String {
    let width = buffer.area.width as usize;
    let height = buffer.area.height as usize;
    let mut out = String::new();
    out.push_str(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>tasknotes-tui snapshot</title>",
    );
    out.push_str(
        "<style>body{margin:0;background:#0f172a;color:#e5e7eb;}\
        .screen{display:inline-block;padding:20px;background:#0f172a;font:16px/1.2 \"Iosevka Term\",\"SFMono-Regular\",Consolas,monospace;}\
        .line{height:1.2em;white-space:pre;}\
        .cell{white-space:pre;}\
        </style></head><body><div class=\"screen\">",
    );
    for y in 0..height {
        out.push_str("<div class=\"line\">");
        let row = &buffer.content[y * width..(y + 1) * width];
        let mut x = 0usize;
        while x < width {
            let cell = &row[x];
            let style = cell.style();
            let mut text = String::new();
            text.push_str(cell.symbol());
            x += 1;
            while x < width {
                let next = &row[x];
                if next.style() != style {
                    break;
                }
                text.push_str(next.symbol());
                x += 1;
            }
            if text.is_empty() {
                text.push(' ');
            }
            out.push_str("<span class=\"cell\"");
            let css = style_to_css(style);
            if !css.is_empty() {
                out.push_str(" style=\"");
                out.push_str(&css);
                out.push('"');
            }
            out.push('>');
            out.push_str(&escape_html(&text));
            out.push_str("</span>");
        }
        out.push_str("</div>");
    }
    out.push_str("</div></body></html>");
    out
}

fn buffer_to_svg(buffer: &Buffer) -> String {
    let width = buffer.area.width as usize;
    let height = buffer.area.height as usize;
    let cell_w = 10usize;
    let cell_h = 20usize;
    let pad = 16usize;
    let svg_w = pad * 2 + width * cell_w;
    let svg_h = pad * 2 + height * cell_h;

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{svg_w}\" height=\"{svg_h}\" viewBox=\"0 0 {svg_w} {svg_h}\">"
    ));
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#0f172a\"/>");
    out.push_str(
        "<g font-family=\"Iosevka Term, SFMono-Regular, Consolas, monospace\" font-size=\"16\">",
    );

    for y in 0..height {
        let row = &buffer.content[y * width..(y + 1) * width];
        for (x, cell) in row.iter().enumerate() {
            let style = cell.style();
            let px = pad + x * cell_w;
            let py = pad + y * cell_h;
            if let Some(bg) = style.bg.and_then(color_to_css) {
                out.push_str(&format!(
                    "<rect x=\"{px}\" y=\"{py}\" width=\"{cell_w}\" height=\"{cell_h}\" fill=\"{bg}\"/>"
                ));
            }
            if style.add_modifier.contains(Modifier::REVERSED) {
                if let Some(fg) = style.fg.and_then(color_to_css) {
                    out.push_str(&format!(
                        "<rect x=\"{px}\" y=\"{py}\" width=\"{cell_w}\" height=\"{cell_h}\" fill=\"{fg}\"/>"
                    ));
                }
            }
        }
    }

    for y in 0..height {
        let row = &buffer.content[y * width..(y + 1) * width];
        for (x, cell) in row.iter().enumerate() {
            let style = cell.style();
            let mut fg = style.fg.and_then(color_to_css);
            let mut bg = style.bg.and_then(color_to_css);
            if style.add_modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let fg = fg.unwrap_or_else(|| "#e5e7eb".to_string());
            let text_x = pad + x * cell_w;
            let text_y = pad + y * cell_h + 15;
            let weight = if style.add_modifier.contains(Modifier::BOLD) {
                " font-weight=\"700\""
            } else {
                ""
            };
            let font_style = if style.add_modifier.contains(Modifier::ITALIC) {
                " font-style=\"italic\""
            } else {
                ""
            };
            let decoration = if style.add_modifier.contains(Modifier::UNDERLINED)
                && style.add_modifier.contains(Modifier::CROSSED_OUT)
            {
                " text-decoration=\"underline line-through\""
            } else if style.add_modifier.contains(Modifier::UNDERLINED) {
                " text-decoration=\"underline\""
            } else if style.add_modifier.contains(Modifier::CROSSED_OUT) {
                " text-decoration=\"line-through\""
            } else {
                ""
            };
            let opacity = if style.add_modifier.contains(Modifier::DIM) {
                " opacity=\"0.7\""
            } else {
                ""
            };
            if style.add_modifier.contains(Modifier::HIDDEN) {
                continue;
            }
            out.push_str(&format!(
                "<text x=\"{text_x}\" y=\"{text_y}\" fill=\"{fg}\" xml:space=\"preserve\" textLength=\"{cell_w}\" lengthAdjust=\"spacingAndGlyphs\"{weight}{font_style}{decoration}{opacity}>{}</text>",
                escape_html(cell.symbol())
            ));
        }
    }

    out.push_str("</g></svg>");
    out
}

fn style_to_css(style: Style) -> String {
    let mut fg = style.fg.and_then(color_to_css);
    let mut bg = style.bg.and_then(color_to_css);
    let mut rules = Vec::new();

    if style.add_modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if let Some(fg) = fg {
        rules.push(format!("color:{fg}"));
    }
    if let Some(bg) = bg {
        rules.push(format!("background:{bg}"));
    }
    if style.add_modifier.contains(Modifier::BOLD) {
        rules.push("font-weight:700".to_string());
    }
    if style.add_modifier.contains(Modifier::DIM) {
        rules.push("opacity:0.7".to_string());
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        rules.push("font-style:italic".to_string());
    }
    let mut decorations = Vec::new();
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        decorations.push("underline");
    }
    if style.add_modifier.contains(Modifier::CROSSED_OUT) {
        decorations.push("line-through");
    }
    if !decorations.is_empty() {
        rules.push(format!("text-decoration:{}", decorations.join(" ")));
    }
    if style.add_modifier.contains(Modifier::HIDDEN) {
        rules.push("visibility:hidden".to_string());
    }
    rules.join(";")
}

fn color_to_css(color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Black => Some("#000000".to_string()),
        Color::Red => Some("#aa0000".to_string()),
        Color::Green => Some("#00aa00".to_string()),
        Color::Yellow => Some("#aa5500".to_string()),
        Color::Blue => Some("#0000aa".to_string()),
        Color::Magenta => Some("#aa00aa".to_string()),
        Color::Cyan => Some("#00aaaa".to_string()),
        Color::Gray => Some("#aaaaaa".to_string()),
        Color::DarkGray => Some("#555555".to_string()),
        Color::LightRed => Some("#ff5555".to_string()),
        Color::LightGreen => Some("#55ff55".to_string()),
        Color::LightYellow => Some("#ffff55".to_string()),
        Color::LightBlue => Some("#5555ff".to_string()),
        Color::LightMagenta => Some("#ff55ff".to_string()),
        Color::LightCyan => Some("#55ffff".to_string()),
        Color::White => Some("#ffffff".to_string()),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Indexed(index) => Some(indexed_color_to_css(index)),
    }
}

fn indexed_color_to_css(index: u8) -> String {
    if index < 16 {
        return match index {
            0 => "#000000",
            1 => "#800000",
            2 => "#008000",
            3 => "#808000",
            4 => "#000080",
            5 => "#800080",
            6 => "#008080",
            7 => "#c0c0c0",
            8 => "#808080",
            9 => "#ff0000",
            10 => "#00ff00",
            11 => "#ffff00",
            12 => "#0000ff",
            13 => "#ff00ff",
            14 => "#00ffff",
            _ => "#ffffff",
        }
        .to_string();
    }
    if index <= 231 {
        let idx = index - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let scale = [0, 95, 135, 175, 215, 255];
        return format!(
            "#{:02x}{:02x}{:02x}",
            scale[r as usize], scale[g as usize], scale[b as usize]
        );
    }
    let gray = 8 + (index - 232) * 10;
    format!("#{gray:02x}{gray:02x}{gray:02x}")
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
