use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use ruff_macros::CacheKey;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(from = "usize", into = "usize")]
pub struct LineWidth {
    width: usize,
    #[serde(skip)]
    column: usize,
    #[serde(skip)]
    tab_size: TabSize,
}

impl Default for LineWidth {
    fn default() -> Self {
        Self::from_line_length(88)
    }
}

impl LineWidth {
    pub fn new(tab_size: TabSize) -> Self {
        LineWidth {
            width: 0,
            column: 0,
            tab_size,
        }
    }

    pub fn from_line_length(line_length: usize) -> Self {
        Self {
            width: line_length,
            column: 0,
            tab_size: TabSize::default(),
        }
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    fn update(mut self, chars: impl Iterator<Item = char>) -> Self {
        let tab_size: usize = self.tab_size.into();
        for c in chars {
            match c {
                '\t' => {
                    let tab_offset = tab_size - (self.column % tab_size);
                    self.width += tab_offset;
                    self.column += tab_offset;
                }
                '\n' | '\r' => {
                    self.width = 0;
                    self.column = 0;
                }
                _ => {
                    self.width += c.width().unwrap_or(0);
                    self.column += 1;
                }
            }
        }
        self
    }

    #[must_use]
    pub fn add_str(self, text: &str) -> Self {
        self.update(text.chars())
    }

    #[must_use]
    pub fn add_char(self, c: char) -> Self {
        self.update(std::iter::once(c))
    }

    #[must_use]
    pub fn add_width(mut self, width: usize) -> Self {
        self.width += width;
        self.column += width;
        self
    }
}

impl PartialOrd<LineWidth> for LineWidth {
    fn partial_cmp(&self, other: &LineWidth) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&other.width)
    }
}

impl Ord for LineWidth {
    fn cmp(&self, other: &LineWidth) -> std::cmp::Ordering {
        self.width.cmp(&other.width)
    }
}

impl PartialEq<LineWidth> for LineWidth {
    fn eq(&self, other: &LineWidth) -> bool {
        self.width == other.width
    }
}

impl Eq for LineWidth {}

impl From<usize> for LineWidth {
    fn from(width: usize) -> Self {
        Self::from_line_length(width)
    }
}

impl From<LineWidth> for usize {
    fn from(line_width: LineWidth) -> usize {
        line_width.width
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TabSize(pub u8);

impl Default for TabSize {
    fn default() -> Self {
        Self(4)
    }
}

impl From<u8> for TabSize {
    fn from(tab_size: u8) -> Self {
        Self(tab_size)
    }
}

impl From<TabSize> for usize {
    fn from(tab_size: TabSize) -> Self {
        tab_size.0 as usize
    }
}
