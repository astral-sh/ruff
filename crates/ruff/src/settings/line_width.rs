use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_width::UnicodeWidthChar;

use ruff_cache::CacheKey;
use ruff_macros::CacheKey;

pub trait LineWidthState: Clone + Default + CacheKey {}

#[derive(Clone, Copy, Debug, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Length;

impl Default for Length {
    fn default() -> Self {
        Self
    }
}

#[derive(Clone, Copy, Default, CacheKey)]
pub struct TabInfos {
    column: usize,
    tab_size: TabSize,
}

impl LineWidthState for Length {}
impl LineWidthState for TabInfos {}

#[derive(Clone, Copy, Debug, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LineWidth<S: LineWidthState> {
    width: usize,
    extra: S,
}

impl Default for LineWidth<Length> {
    fn default() -> Self {
        Self {
            width: 88,
            extra: Length,
        }
    }
}

impl Default for LineWidth<TabInfos> {
    fn default() -> Self {
        Self::new(TabSize::default())
    }
}

impl<S> LineWidth<S>
where
    S: LineWidthState,
{
    pub const fn width(&self) -> usize {
        self.width
    }
}

impl LineWidth<TabInfos> {
    pub fn new(tab_size: TabSize) -> Self {
        LineWidth {
            width: 0,
            extra: TabInfos {
                column: 0,
                tab_size,
            },
        }
    }

    fn update(mut self, chars: impl Iterator<Item = char>) -> Self {
        let tab_size: usize = self.extra.tab_size.into();
        for c in chars {
            match c {
                '\t' => {
                    let tab_offset = tab_size - (self.extra.column % tab_size);
                    self.width += tab_offset;
                    self.extra.column += tab_offset;
                }
                '\n' | '\r' => {
                    self.width = 0;
                    self.extra.column = 0;
                }
                _ => {
                    self.width += c.width().unwrap_or(0);
                    self.extra.column += 1;
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
        self.extra.column += width;
        self
    }
}

impl<S> PartialOrd<LineWidth<S>> for LineWidth<S>
where
    S: LineWidthState,
{
    fn partial_cmp(&self, other: &LineWidth<S>) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&other.width)
    }
}

impl PartialOrd<LineWidth<Length>> for LineWidth<TabInfos> {
    fn partial_cmp(&self, other: &LineWidth<Length>) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&other.width)
    }
}

impl<S> Ord for LineWidth<S>
where
    S: LineWidthState,
{
    fn cmp(&self, other: &LineWidth<S>) -> std::cmp::Ordering {
        self.width.cmp(&other.width)
    }
}

impl<S> PartialEq<LineWidth<S>> for LineWidth<S>
where
    S: LineWidthState,
{
    fn eq(&self, other: &LineWidth<S>) -> bool {
        self.width == other.width
    }
}

impl PartialEq<LineWidth<Length>> for LineWidth<TabInfos> {
    fn eq(&self, other: &LineWidth<Length>) -> bool {
        self.width == other.width
    }
}

impl<S> Eq for LineWidth<S> where S: LineWidthState {}

impl From<usize> for LineWidth<Length> {
    fn from(width: usize) -> Self {
        Self {
            width,
            ..Default::default()
        }
    }
}

impl<'de> Deserialize<'de> for LineWidth<Length> {
    fn deserialize<D>(deserializer: D) -> Result<LineWidth<Length>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let width = usize::deserialize(deserializer)?;
        Ok(LineWidth {
            width,
            ..Default::default()
        })
    }
}

impl Serialize for LineWidth<Length> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        usize::serialize(&self.width, serializer)
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

pub type LineLength = LineWidth<Length>;
pub type Width = LineWidth<TabInfos>;
