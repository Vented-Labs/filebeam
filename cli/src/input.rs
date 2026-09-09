use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use zeroize::Zeroize;

use crate::presentation::{clean, clip};

#[derive(Default)]
pub struct Input {
    pub value: String,
    cursor: usize,
}

impl Input {
    pub fn new(value: String) -> Self {
        Self {
            cursor: value.len(),
            value,
        }
    }

    pub fn insert(&mut self, text: &str) {
        let text = clean(text);
        if self.value.len() + text.len() > 8192 {
            return;
        }
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }

    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.value)
    }

    pub fn handle(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('u') => {
                    self.value.zeroize();
                    self.cursor = 0;
                }
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.value.len(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(&c.to_string())
            }
            KeyCode::Left => self.cursor = self.previous(),
            KeyCode::Right => self.cursor = self.next(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            KeyCode::Backspace => {
                let previous = self.previous();
                self.value.replace_range(previous..self.cursor, "");
                self.cursor = previous;
            }
            KeyCode::Delete => {
                self.value.replace_range(self.cursor..self.next(), "");
            }
            _ => {}
        }
    }

    fn previous(&self) -> usize {
        self.value[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.value[self.cursor..]
            .graphemes(true)
            .next()
            .map(|grapheme| self.cursor + grapheme.len())
            .unwrap_or(self.cursor)
    }

    pub fn visible(&self, width: u16, masked: bool) -> (String, u16) {
        if width == 0 {
            return (String::new(), 0);
        }
        let cursor = self.value[..self.cursor].graphemes(true).count();
        let graphemes = self
            .value
            .graphemes(true)
            .map(|g| if masked { "*" } else { g })
            .collect::<Vec<_>>();
        let mut start = 0;
        while start < cursor && graphemes[start..cursor].concat().width() >= width as usize {
            start += 1;
        }
        let column = graphemes[start..cursor].concat().width() as u16;
        (clip(&graphemes[start..].concat(), width), column)
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_and_secret_scrolling_respect_graphemes() {
        let mut input = Input::new("a\u{301}日本".into());
        input.handle(KeyCode::Left.into());
        input.handle(KeyCode::Backspace.into());
        assert_eq!(input.value, "a\u{301}本");
        input.handle(KeyCode::End.into());
        let (visible, column) = input.visible(2, true);
        assert_eq!(visible, "*");
        assert_eq!(column, 1);
        input.handle(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert!(input.value.is_empty());
    }
}
