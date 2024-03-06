use bitflags::bitflags;

use ruff_text_size::{TextLen, TextSize};

bitflags! {
    /// [String and Bytes literals]: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
    /// [PEP 701]: https://peps.python.org/pep-0701/
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct StringFlagsInner: u8 {
        /// The string uses double quotes (`"`).
        /// If this flag is not set, the string uses single quotes (`'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set.
        const U_PREFIX = 1 << 2;

        /// The string has a `b` or `B` prefix.
        /// This means that the string is a sequence of `int`s at runtime,
        /// rather than a sequence of `str`s.
        /// Strings with this flag can also be raw strings,
        /// but can have no other prefixes.
        const B_PREFIX = 1 << 3;

        /// The string has a `f` or `F` prefix, meaning it is an f-string.
        /// F-strings can also be raw strings,
        /// but can have no other prefixes.
        const F_PREFIX = 1 << 4;

        /// The string has an `r` or `R` prefix, meaning it is a raw string.
        /// F-strings and byte-strings can be raw,
        /// as can strings with no other prefixes.
        /// U-strings cannot be raw.
        const R_PREFIX = 1 << 5;
    }
}

impl TryFrom<char> for StringFlagsInner {
    type Error = String;

    fn try_from(value: char) -> Result<Self, String> {
        let result = match value {
            '\'' => Self::empty(),
            '"' => Self::DOUBLE,
            'r' | 'R' => Self::R_PREFIX,
            'u' | 'U' => Self::U_PREFIX,
            'b' | 'B' => Self::B_PREFIX,
            'f' | 'F' => Self::F_PREFIX,
            _ => return Err(format!("Unexpected prefix or quote '{value}'")),
        };
        Ok(result)
    }
}

const DISALLOWED_U_STRING_PREFIXES: StringFlagsInner = StringFlagsInner::B_PREFIX
    .union(StringFlagsInner::F_PREFIX)
    .union(StringFlagsInner::R_PREFIX);

const DISALLOWED_B_STRING_PREFIXES: StringFlagsInner =
    StringFlagsInner::U_PREFIX.union(StringFlagsInner::F_PREFIX);

const DISALLOWED_F_STRING_PREFIXES: StringFlagsInner =
    StringFlagsInner::U_PREFIX.union(StringFlagsInner::B_PREFIX);

const DISALLOWED_R_STRING_PREFIXES: StringFlagsInner = StringFlagsInner::U_PREFIX;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringFlags(StringFlagsInner);

impl TryFrom<char> for StringFlags {
    type Error = String;

    fn try_from(value: char) -> Result<Self, String> {
        Ok(Self(StringFlagsInner::try_from(value)?))
    }
}

impl StringFlags {
    pub const fn is_raw(self) -> bool {
        self.0.contains(StringFlagsInner::R_PREFIX)
    }

    pub const fn is_bytestring(self) -> bool {
        self.0.contains(StringFlagsInner::B_PREFIX)
    }

    pub const fn is_fstring(self) -> bool {
        self.0.contains(StringFlagsInner::F_PREFIX)
    }

    pub const fn is_ustring(self) -> bool {
        self.0.contains(StringFlagsInner::U_PREFIX)
    }

    pub const fn quote_style(self) -> QuoteStyle {
        if self.0.contains(StringFlagsInner::DOUBLE) {
            QuoteStyle::Double
        } else {
            QuoteStyle::Single
        }
    }

    pub const fn is_triple_quoted(self) -> bool {
        self.0.contains(StringFlagsInner::TRIPLE_QUOTED)
    }

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

    pub const fn prefix_str(self) -> &'static str {
        if self.is_ustring() {
            return "u";
        }
        if self.is_bytestring() {
            if self.is_raw() {
                return "rb";
            }
            return "b";
        }
        if self.is_fstring() {
            if self.is_raw() {
                return "rf";
            }
            return "f";
        }
        if self.is_raw() {
            return "r";
        }
        ""
    }

    pub fn prefix_len(self) -> TextSize {
        self.prefix_str().text_len()
    }

    pub fn quote_len(self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::from(3)
        } else {
            TextSize::from(1)
        }
    }

    #[must_use]
    pub fn with_double_quotes(mut self) -> Self {
        self.0 |= StringFlagsInner::DOUBLE;
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= StringFlagsInner::TRIPLE_QUOTED;
        self
    }

    pub fn with_u_prefix(mut self) -> Result<Self, &'static str> {
        if self.0.intersects(DISALLOWED_U_STRING_PREFIXES) {
            Err("U-strings cannot have any other prefixes set")
        } else {
            self.0 |= StringFlagsInner::U_PREFIX;
            Ok(self)
        }
    }

    pub fn with_b_prefix(mut self) -> Result<Self, &'static str> {
        if self.0.intersects(DISALLOWED_B_STRING_PREFIXES) {
            Err("Bytestrings cannot have the `u` or `f` prefix also set")
        } else {
            self.0 |= StringFlagsInner::B_PREFIX;
            Ok(self)
        }
    }

    pub fn with_f_prefix(mut self) -> Result<Self, &'static str> {
        if self.0.intersects(DISALLOWED_F_STRING_PREFIXES) {
            Err("F-strings cannot have the `b` or `u` prefix also set")
        } else {
            self.0 |= StringFlagsInner::F_PREFIX;
            Ok(self)
        }
    }

    pub fn with_r_prefix(mut self) -> Result<Self, &'static str> {
        if self.0.intersects(DISALLOWED_R_STRING_PREFIXES) {
            Err("Raw strings cannot have the `u` prefix also set")
        } else {
            self.0 |= StringFlagsInner::R_PREFIX;
            Ok(self)
        }
    }
}

/// TODO: Use this enum in `crates/ruff_python_formatter/src/string/mod.rs`?
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
