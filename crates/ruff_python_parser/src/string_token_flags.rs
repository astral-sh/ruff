use bitflags::bitflags;

use ruff_text_size::{TextLen, TextSize};

bitflags! {
    /// The kind of quote used for a string
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct QuoteFlags: u8 {
        /// The string uses double quotes (`"`).
        /// If this flag is not set, the string uses single quotes (`'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;
    }
}

/// [String and Bytes literals]: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
/// [PEP 701]: https://peps.python.org/pep-0701/
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StringPrefix {
    /// The string has a `u` or `U` prefix.
    /// While this prefix is a no-op at runtime,
    /// strings with this prefix can have no other prefixes set.
    U,

    /// The string has an `r` or `R` prefix, meaning it is a raw string.
    /// F-strings and byte-strings can be raw,
    /// as can strings with no other prefixes.
    /// U-strings cannot be raw.
    R,

    /// The string has a `f` or `F` prefix, meaning it is an f-string.
    /// F-strings can also be raw strings,
    /// but can have no other prefixes.
    F,

    /// The string has a `b` or `B` prefix.
    /// This means that the string is a sequence of `int`s at runtime,
    /// rather than a sequence of `str`s.
    /// Bytestrings can also be raw strings,
    /// but can have no other prefixes.
    B,

    /// A string that has has any one of the prefixes
    /// `{"rf", "rF", "Rf", "RF", "fr", "fR", "Fr", "FR"}`
    /// Semantically, these all have the same meaning:
    /// the string is both an f-string and a raw-string
    RF,

    /// A string that has has any one of the prefixes
    /// `{"rb", "rB", "Rb", "RB", "br", "bR", "Br", "BR"}`
    /// Semantically, these all have the same meaning:
    /// the string is both an bytestring and a raw-string
    RB,
}

impl TryFrom<char> for StringPrefix {
    type Error = String;

    fn try_from(value: char) -> Result<Self, String> {
        let result = match value {
            'r' | 'R' => Self::R,
            'u' | 'U' => Self::U,
            'b' | 'B' => Self::B,
            'f' | 'F' => Self::F,
            _ => return Err(format!("Unexpected prefix '{value}'")),
        };
        Ok(result)
    }
}

impl TryFrom<[char; 2]> for StringPrefix {
    type Error = String;

    fn try_from(value: [char; 2]) -> Result<Self, String> {
        match value {
            ['r' | 'R', 'f' | 'F'] | ['f' | 'F', 'r' | 'R'] => Ok(Self::RF),
            ['r' | 'R', 'b' | 'B'] | ['b' | 'B', 'r' | 'R'] => Ok(Self::RB),
            _ => Err(format!("Unexpected prefix '{}{}'", value[0], value[1])),
        }
    }
}

impl StringPrefix {
    const fn as_str(self) -> &'static str {
        match self {
            Self::U => "u",
            Self::B => "b",
            Self::F => "f",
            Self::R => "r",
            Self::RB => "rb",
            Self::RF => "rf",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringKind {
    quote_flags: QuoteFlags,
    prefix: Option<StringPrefix>,
}

impl StringKind {
    pub(crate) const fn with_prefix(prefix: StringPrefix) -> Self {
        Self {
            quote_flags: QuoteFlags::empty(),
            prefix: Some(prefix),
        }
    }

    /// Does the string have any prefixes?
    pub const fn has_prefix(self) -> bool {
        self.prefix.is_some()
    }

    /// Does the string have a `u` or `U` prefix?
    pub const fn is_ustring(self) -> bool {
        matches!(self.prefix, Some(StringPrefix::U))
    }

    /// Does the string have an `r` or `R` prefix?
    pub const fn is_rawstring(self) -> bool {
        matches!(
            self.prefix,
            Some(StringPrefix::R | StringPrefix::RF | StringPrefix::RB)
        )
    }

    /// Does the string have an `f` or `F` prefix?
    pub const fn is_fstring(self) -> bool {
        matches!(self.prefix, Some(StringPrefix::F | StringPrefix::RF))
    }

    /// Does the string have a `b` or `B` prefix?
    pub const fn is_bytestring(self) -> bool {
        matches!(self.prefix, Some(StringPrefix::B | StringPrefix::RB))
    }

    /// Does the string use single or double quotes in its opener and closer?
    pub const fn quote_style(self) -> QuoteStyle {
        if self.quote_flags.contains(QuoteFlags::DOUBLE) {
            QuoteStyle::Double
        } else {
            QuoteStyle::Single
        }
    }

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    pub const fn is_triple_quoted(self) -> bool {
        self.quote_flags.contains(QuoteFlags::TRIPLE_QUOTED)
    }

    /// A `str` representation of the quotes used to start and close.
    /// This does not include any prefixes the string has in its opener.
    pub const fn quote_str(self) -> &'static str {
        if self.is_triple_quoted() {
            match self.quote_style() {
                QuoteStyle::Single => "'''",
                QuoteStyle::Double => r#"""""#,
            }
        } else {
            match self.quote_style() {
                QuoteStyle::Single => "'",
                QuoteStyle::Double => "\"",
            }
        }
    }

    /// A `str` representation of the prefixes used (if any)
    /// in the string's opener.
    pub const fn prefix_str(self) -> &'static str {
        if let Some(prefix) = self.prefix {
            prefix.as_str()
        } else {
            ""
        }
    }

    /// The length of the prefixes used (if any) in the string's opener.
    pub fn prefix_len(self) -> TextSize {
        self.prefix_str().text_len()
    }

    /// The length of the quotes used to start and close the string.
    /// This does not include the length of any prefixes the string has
    /// in its opener.
    pub const fn quote_len(self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }

    /// The total length of the string's opener,
    /// i.e., the length of the prefixes plus the length
    /// of the quotes used to open the string.
    pub fn opener_len(self) -> TextSize {
        self.prefix_len() + self.quote_len()
    }

    /// The total length of the string's closer.
    /// This is always equal to `self.quote_len()`,
    /// but is provided here for symmetry with the `opener_len()` method.
    pub const fn closer_len(self) -> TextSize {
        self.quote_len()
    }

    pub fn format_string_contents(self, contents: &str) -> String {
        format!(
            "{}{}{}{}",
            self.prefix_str(),
            self.quote_str(),
            contents,
            self.quote_str()
        )
    }

    #[must_use]
    pub fn with_double_quotes(mut self) -> Self {
        self.quote_flags |= QuoteFlags::DOUBLE;
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.quote_flags |= QuoteFlags::TRIPLE_QUOTED;
        self
    }
}

#[derive(Debug, Default, Copy, Clone, Hash, PartialEq, Eq)]
pub enum QuoteStyle {
    /// E.g. '
    Single,
    /// E.g. "
    #[default]
    Double,
}

impl QuoteStyle {
    pub const fn as_char(self) -> char {
        match self {
            Self::Single => '\'',
            Self::Double => '"',
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Single => Self::Double,
            Self::Double => Self::Single,
        }
    }
}
