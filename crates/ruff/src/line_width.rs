use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use ruff_macros::CacheKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LineLength(usize);

impl Default for LineLength {
    fn default() -> Self {
        Self(88)
    }
}

impl LineLength {
    pub const fn get(&self) -> usize {
        self.0
    }
}

impl From<usize> for LineLength {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineWidth {
    /// The current width of the line.
    width: usize,
    /// The current column of the line.
    column: usize,
    /// The tab width.
    tab_size: TabSize,
}

impl Default for LineWidth {
    fn default() -> Self {
        Self::new(TabSize::default())
    }
}

impl PartialEq for LineWidth {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
    }
}

impl Eq for LineWidth {}

impl PartialOrd for LineWidth {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&other.width)
    }
}

impl Ord for LineWidth {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.width.cmp(&other.width)
    }
}

impl LineWidth {
    pub fn get(&self) -> usize {
        self.width
    }

    pub fn new(tab_size: TabSize) -> Self {
        LineWidth {
            width: 0,
            column: 0,
            tab_size,
        }
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

impl PartialEq<LineLength> for LineWidth {
    fn eq(&self, other: &LineLength) -> bool {
        self.width == other.0
    }
}

impl PartialOrd<LineLength> for LineWidth {
    fn partial_cmp(&self, other: &LineLength) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&other.0)
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
