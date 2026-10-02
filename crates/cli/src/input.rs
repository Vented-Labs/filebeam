use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use zeroize::Zeroize;

use crate::presentation::{clean, clip};

#[derive(Clone, Default)]
pub struct Input {
    pub value: String,
    cursor: usize,
    multiline: bool,
}

impl Input {
    pub fn new(value: String) -> Self {
        Self {
            cursor: value.len(),
            value,
            multiline: false,
        }
    }

    pub fn insert(&mut self, text: &str) {
        self.try_insert(text);
    }

    pub fn multiline() -> Self {
        Self {
            multiline: true,
            value: String::new(),
            cursor: 0,
        }
    }

    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn try_insert(&mut self, text: &str) -> bool {
        let text = if self.multiline {
            text.to_owned()
        } else {
            clean(text)
        };
        // Native note editing accepts the same bounded 64 KiB schema as file/stdin input.
        if self.value.len() + text.len() > 64 * 1024 {
            return false;
        }
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
        true
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
            KeyCode::Up if self.multiline => self.move_line(false),
            KeyCode::Down if self.multiline => self.move_line(true),
            KeyCode::Enter if self.multiline => self.insert("\n"),
            KeyCode::Tab if self.multiline => self.insert("\t"),
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

    fn move_line(&mut self, down: bool) {
        let start = self.value[..self.cursor].rfind('\n').map_or(0, |n| n + 1);
        let column = self.value[start..self.cursor].graphemes(true).count();
        let target = if down {
            let Some(next) = self.value[self.cursor..].find('\n') else {
                return;
            };
            self.cursor + next + 1
        } else {
            if start == 0 {
                return;
            }
            self.value[..start - 1].rfind('\n').map_or(0, |n| n + 1)
        };
        let line = self.value[target..].split('\n').next().unwrap_or("");
        self.cursor = target
            + line
                .graphemes(true)
                .take(column)
                .map(str::len)
                .sum::<usize>();
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
    fn multiline_paste_preserves_content_and_rejects_overflow_atomically() {
        let mut input = Input::multiline();
        let text = "日本\n\tsecond\r\n\u{1b}[31m";
        assert!(input.try_insert(text));
        assert_eq!(input.value, text);
        assert!(!input.try_insert(&"x".repeat(65536)));
        assert_eq!(input.value, text);
        let mut editing = Input::multiline();
        editing.insert("ab\ncd");
        editing.handle(KeyCode::Up.into());
        editing.handle(KeyCode::Enter.into());
        assert_eq!(editing.value, "ab\n\ncd");
    }
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
