use std::{env, time::Duration};

use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::config::Config;

#[derive(Clone, Copy)]
enum Colors {
    True,
    Indexed,
    None,
}

#[derive(Clone, Copy)]
pub struct Theme {
    colors: Colors,
    pub motion: bool,
    pub unicode: bool,
}

impl Theme {
    pub fn new(config: &Config) -> Self {
        let term = env::var("TERM").unwrap_or_default();
        let color_term = env::var("COLORTERM").unwrap_or_default();
        let colors = if config.no_color || env::var_os("NO_COLOR").is_some() || term == "dumb" {
            Colors::None
        } else if matches!(color_term.as_str(), "truecolor" | "24bit") || term.contains("direct") {
            Colors::True
        } else {
            Colors::Indexed
        };
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| env::var(name).ok().filter(|v| !v.is_empty()));
        let unicode =
            term != "dumb" && locale.is_none_or(|locale| locale != "C" && locale != "POSIX");
        Self {
            colors,
            motion: !config.reduced_motion && term != "dumb",
            unicode,
        }
    }

    pub fn color(self, rgb: (u8, u8, u8), index: u8) -> Color {
        match self.colors {
            Colors::True => Color::Rgb(rgb.0, rgb.1, rgb.2),
            Colors::Indexed => Color::Indexed(index),
            Colors::None => Color::Reset,
        }
    }
    pub fn bg(self) -> Color {
        self.color((11, 9, 20), 233)
    }
    pub fn surface(self) -> Color {
        self.color((19, 16, 32), 234)
    }
    pub fn raised(self) -> Color {
        self.color((27, 21, 43), 235)
    }
    pub fn border(self) -> Color {
        self.color((53, 39, 71), 60)
    }
    pub fn text(self) -> Color {
        self.color((247, 245, 255), 255)
    }
    pub fn muted(self) -> Color {
        self.color((170, 160, 192), 146)
    }
    pub fn accent(self) -> Color {
        self.color((192, 132, 252), 183)
    }
    pub fn action(self) -> Color {
        self.color((124, 58, 237), 99)
    }
    pub fn selected(self) -> Color {
        self.color((38, 22, 63), 54)
    }
    pub fn success(self) -> Color {
        self.color((110, 231, 183), 115)
    }
    pub fn danger(self) -> Color {
        self.color((253, 164, 175), 217)
    }
    pub fn warning(self) -> Color {
        self.color((252, 211, 77), 221)
    }
    pub fn base(self) -> Style {
        Style::default().fg(self.text()).bg(self.bg())
    }
    pub fn strong(self) -> Style {
        Style::default()
            .fg(self.text())
            .add_modifier(Modifier::BOLD)
    }
    pub fn dim(self) -> Style {
        Style::default().fg(self.muted())
    }
    pub fn symbol<'a>(self, unicode: &'a str, ascii: &'a str) -> &'a str {
        if self.unicode { unicode } else { ascii }
    }
    pub fn gradient(self, position: f64) -> Color {
        let position = position.clamp(0.0, 1.0);
        self.color(
            (
                lerp(124, 199, position),
                lerp(58, 91, position),
                lerp(237, 250, position),
            ),
            [99, 135, 171, 177, 213][(position * 4.0) as usize],
        )
    }

    fn highlight(self, position: f64, amount: f64) -> Color {
        self.color(
            (
                lerp(lerp(124, 199, position), 231, amount),
                lerp(lerp(58, 91, position), 161, amount),
                lerp(lerp(237, 250, position), 255, amount),
            ),
            if amount > 0.5 {
                219
            } else {
                [99, 135, 171, 177, 213][(position * 4.0).min(4.0) as usize]
            },
        )
    }

    #[cfg(test)]
    pub fn fixture() -> Self {
        Self {
            colors: Colors::True,
            motion: false,
            unicode: true,
        }
    }
}

fn lerp(from: u8, to: u8, amount: f64) -> u8 {
    (from as f64 + (to as f64 - from as f64) * amount).round() as u8
}

pub fn clean(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

pub fn clip(value: &str, width: u16) -> String {
    let value = clean(value);
    if value.width() <= width as usize {
        return value;
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for grapheme in value.graphemes(true) {
        let length = grapheme.width();
        if used + length > width.saturating_sub(1) as usize {
            break;
        }
        result.push_str(grapheme);
        used += length;
    }
    result.push('…');
    result
}

pub fn bytes(value: u64) -> String {
    if value < 1024 {
        return format!("{value} B");
    }
    let power = ((value as f64).log(1024.0).floor() as usize).min(4);
    format!(
        "{:.1} {}",
        value as f64 / 1024_f64.powi(power as i32),
        ["B", "KiB", "MiB", "GiB", "TiB"][power]
    )
}

pub fn wrap(value: &str, width: u16) -> Vec<String> {
    let mut rows = Vec::new();
    for line in value.lines() {
        let line = clean(line);
        let mut row = String::new();
        let mut used = 0;
        for grapheme in line.graphemes(true) {
            let length = grapheme.width();
            if used + length > width.max(1) as usize {
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
            row.push_str(grapheme);
            used += length;
        }
        rows.push(row);
    }
    rows
}

pub fn duration(value: Duration) -> String {
    let seconds = value.as_secs();
    if seconds < 60 {
        format!("{:.1}s", value.as_secs_f64())
    } else if seconds < 3600 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60)
    }
}

pub fn line(area: Rect, buffer: &mut Buffer, value: impl Into<Line<'static>>) {
    Paragraph::new(value.into()).render(area, buffer);
}

pub fn at(area: Rect, row: u16, height: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(row).min(area.bottom()),
        area.width,
        height.min(area.height.saturating_sub(row)),
    )
}

pub fn card(area: Rect, buffer: &mut Buffer, theme: Theme, focused: bool) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if theme.unicode {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(Style::default().fg(if focused {
            theme.accent()
        } else {
            theme.border()
        }))
        .style(Style::default().bg(theme.surface()));
    let inner = block
        .inner(area)
        .inner(Margin::new(2, if area.height >= 12 { 1 } else { 0 }));
    block.render(area, buffer);
    inner
}

pub fn button(area: Rect, buffer: &mut Buffer, theme: Theme, label: &str, focused: bool) {
    let style = if focused {
        theme
            .strong()
            .bg(theme.action())
            .add_modifier(Modifier::BOLD)
    } else {
        theme.strong().bg(theme.selected())
    };
    Paragraph::new(clip(label, area.width.saturating_sub(2)))
        .centered()
        .style(style)
        .block(
            Block::default().padding(ratatui::widgets::Padding::vertical(if area.height >= 3 {
                1
            } else {
                0
            })),
        )
        .render(area, buffer);
}

pub fn key(theme: Theme, name: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!(" {name} "), theme.strong().bg(theme.raised())),
        Span::styled(format!(" {label}   "), theme.dim()),
    ]
}

pub fn spinner(theme: Theme, elapsed: Duration) -> &'static str {
    if !theme.motion {
        return theme.symbol("·", "-");
    }
    let index = elapsed.as_millis() as usize / 100;
    if theme.unicode {
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][index % 10]
    } else {
        ["-", "\\", "|", "/"][index % 4]
    }
}

pub fn meter(area: Rect, buffer: &mut Buffer, theme: Theme, ratio: Option<f64>, elapsed: Duration) {
    if area.is_empty() {
        return;
    }
    let width = area.width as f64;
    let amount = ratio.unwrap_or(0.0).clamp(0.0, 1.0) * width;
    let sweep = if theme.motion {
        (elapsed.as_secs_f64() * 9.0) % (width + 8.0) - 4.0
    } else {
        width / 2.0
    };
    for x in 0..area.width {
        let distance = amount - x as f64;
        let moving = ratio.is_none() && (x as f64 - sweep).abs() < 3.0;
        let filled = distance >= 1.0 || moving;
        let partial = if theme.unicode && distance > 0.0 && distance < 1.0 {
            ["▏", "▎", "▍", "▌", "▋", "▊", "▉"][(distance * 7.0).floor().min(6.0) as usize]
        } else {
            theme.symbol("━", "-")
        };
        let symbol = if filled {
            theme.symbol("━", "=")
        } else {
            partial
        };
        let color = if filled || distance > 0.0 {
            let position = x as f64 / width;
            let cycle = elapsed.as_secs_f64() % 4.8;
            let sheen = if theme.motion && cycle > 3.2 {
                (1.0 - (position - (cycle - 3.2) / 1.6).abs() * 12.0).max(0.0) * 0.7
            } else {
                0.0
            };
            theme.highlight(
                position,
                if distance > 0.0 && distance < 1.5 {
                    0.6
                } else {
                    sheen
                },
            )
        } else {
            theme.border()
        };
        for y in area.y..area.bottom() {
            if let Some(cell) = buffer.cell_mut((area.x + x, y)) {
                cell.set_symbol(symbol).set_fg(color);
            }
        }
    }
}

/// A cell-sized interpretation of the folded F from the website's identity asset.
pub fn mark(area: Rect, buffer: &mut Buffer, theme: Theme) {
    let rows = [
        "        ▄▄██",
        "    ▄▄████▀▀",
        "▄▄████▀▀    ",
        "████▄▄      ",
        "██▀▀████▄   ",
        "██▄   ▀▀    ",
        "▀███▄       ",
    ];
    if !theme.unicode {
        Paragraph::new("FILEBEAM")
            .style(theme.strong().fg(theme.accent()))
            .render(area, buffer);
        return;
    }
    for (y, row) in rows.iter().enumerate().take(area.height as usize) {
        for (x, symbol) in row.chars().enumerate().take(area.width as usize) {
            if symbol != ' ' {
                let color = if y == 3 {
                    theme.color((231, 161, 255), 219)
                } else {
                    theme.gradient(x as f64 / 11.0)
                };
                if let Some(cell) = buffer.cell_mut((area.x + x as u16, area.y + y as u16)) {
                    cell.set_char(symbol).set_fg(color);
                }
            }
        }
    }
}

pub fn ambient(area: Rect, buffer: &mut Buffer, theme: Theme, elapsed: Duration) {
    let pulse = if theme.motion {
        0.9 + 0.1 * (elapsed.as_secs_f64() / 3.0).sin()
    } else {
        1.0
    };
    for x in 0..area.width {
        for y in 0..area.height {
            let dx = (x as f64 - 16.0) / 35.0;
            let dy = y as f64 / 5.0;
            let glow = (-dx * dx - dy * dy).exp() * pulse;
            let color = theme.color(
                (lerp(11, 23, glow), lerp(9, 12, glow), lerp(20, 38, glow)),
                if glow > 0.55 { 234 } else { 233 },
            );
            if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                cell.set_bg(color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cannot_inject_terminal_controls_or_overflow_cells() {
        assert!(!clean("report\u{1b}[2J\n").contains('\u{1b}'));
        assert_eq!(clip("日本語-document.txt", 7).width(), 7);
        assert!(clip("a\u{301}bc", 2).width() <= 2);
    }
}
