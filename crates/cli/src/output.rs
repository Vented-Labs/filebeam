use std::io::{self, IsTerminal, Write};

use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition},
    queue,
    style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use ratatui::layout::Rect;

use crate::presentation::{Theme, clean, clip};

pub struct Hyperlink {
    pub area: Rect,
    pub target: String,
    pub label: String,
}

impl Hyperlink {
    pub fn new(area: Rect, target: &str) -> Option<Self> {
        sequence(target, "")?;
        Some(Self {
            area,
            target: target.to_owned(),
            label: clip(target, area.width),
        })
    }
}

/// OSC 8 carries the entire URI (including the key fragment) independently of visible line wrapping.
fn sequence(target: &str, label: &str) -> Option<String> {
    let url = reqwest::Url::parse(target).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || clean(target) != target
    {
        return None;
    }
    Some(format!(
        "\x1b]8;;{target}\x1b\\{}\x1b]8;;\x1b\\",
        clean(label)
    ))
}

pub fn result(value: &str, plain: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if !plain
        && stdout.is_terminal()
        && std::env::var("TERM").unwrap_or_default() != "dumb"
        && let Some(link) = sequence(value, value)
    {
        return writeln!(stdout, "{link}");
    }
    writeln!(stdout, "{value}")
}

pub fn screen_link(output: &mut impl Write, link: &Hyperlink, theme: Theme) -> io::Result<()> {
    if link.area.is_empty() {
        return Ok(());
    }
    if let Some(value) = sequence(&link.target, &link.label) {
        queue!(
            output,
            SavePosition,
            MoveTo(link.area.x, link.area.y),
            SetForegroundColor(theme.accent().into()),
            SetBackgroundColor(theme.selected().into()),
            Print(value),
            ResetColor,
            RestorePosition
        )?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortened_labels_keep_the_full_key_in_the_click_target() {
        let target = "http://localhost:8000/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.entire-key";
        let link = Hyperlink::new(Rect::new(1, 1, 20, 1), target).unwrap();
        assert!(link.label.ends_with('…'));
        let encoded = sequence(&link.target, &link.label).unwrap();
        assert!(encoded.starts_with(&format!("\x1b]8;;{target}\x1b\\")));
        assert!(sequence("https://example.org/\x1b]bad", "link").is_none());
    }
}
