use std::fmt;

use bitflags::bitflags;

use ruff_python_ast::{str::Quote, FStringPrefix, StringLiteralPrefix};
use ruff_text_size::{TextLen, TextSize};

bitflags! {
    /// Flags that can be queried to obtain information
    /// regarding the prefixes and quotes used for a string literal.
    ///
    /// Note that not all of these flags can be validly combined -- e.g.,
    /// it is invalid to combine the `U_PREFIX` flag with any other
    /// of the `*_PREFIX` flags. As such, the recommended way to set the
    /// prefix flags is by calling the `as_flags()` method on the
    /// `StringPrefix` enum.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct StringFlags: u8 {
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

        /// The string has an `r` prefix, meaning it is a raw string.
        /// F-strings and byte-strings can be raw,
        /// as can strings with no other prefixes.
        /// U-strings cannot be raw.
        const R_PREFIX_LOWER = 1 << 5;

        /// The string has an `R` prefix, meaning it is a raw string.
        /// The casing of the `r`/`R` has no semantic significance at runtime;
        /// see https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 6;
    }
}

/// Enumeration of all the possible valid prefixes
/// prior to a Python string literal.
///
/// Using the `as_flags()` method on variants of this enum
/// is the recommended way to set `*_PREFIX` flags from the
/// `StringFlags` bitflag, as it means that you cannot accidentally
/// set a combination of `*_PREFIX` flags that would be invalid
/// at runtime in Python.
///
/// [String and Bytes literals]: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
/// [PEP 701]: https://peps.python.org/pep-0701/
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StringPrefix {
    /// The string has a `u` or `U` prefix.
    /// While this prefix is a no-op at runtime,
    /// strings with this prefix can have no other prefixes set.
    Unicode,

    /// The string has an `r` or `R` prefix, meaning it is a raw string.
    /// F-strings and byte-strings can be raw,
    /// as can strings with no other prefixes.
    /// U-strings cannot be raw.
    Raw { uppercase: bool },

    /// The string has a `f` or `F` prefix, meaning it is an f-string.
    /// F-strings can also be raw strings,
    /// but can have no other prefixes.
    Format,

    /// The string has a `b` or `B` prefix.
    /// This means that the string is a sequence of `int`s at runtime,
    /// rather than a sequence of `str`s.
    /// Bytestrings can also be raw strings,
    /// but can have no other prefixes.
    Bytes,

    /// A string that has has any one of the prefixes
    /// `{"rf", "rF", "Rf", "RF", "fr", "fR", "Fr", "FR"}`
    /// Semantically, these all have the same meaning:
    /// the string is both an f-string and a raw-string
    RawFormat { uppercase_r: bool },

    /// A string that has has any one of the prefixes
    /// `{"rb", "rB", "Rb", "RB", "br", "bR", "Br", "BR"}`
    /// Semantically, these all have the same meaning:
    /// the string is both an bytestring and a raw-string
    RawBytes { uppercase_r: bool },
}

impl TryFrom<char> for StringPrefix {
    type Error = String;

    fn try_from(value: char) -> Result<Self, String> {
        let result = match value {
            'r' => Self::Raw { uppercase: false },
            'R' => Self::Raw { uppercase: true },
            'u' | 'U' => Self::Unicode,
            'b' | 'B' => Self::Bytes,
            'f' | 'F' => Self::Format,
            _ => return Err(format!("Unexpected prefix '{value}'")),
        };
        Ok(result)
    }
}

impl TryFrom<[char; 2]> for StringPrefix {
    type Error = String;

    fn try_from(value: [char; 2]) -> Result<Self, String> {
        let result = match value {
            ['r', 'f' | 'F'] | ['f' | 'F', 'r'] => Self::RawFormat { uppercase_r: false },
            ['R', 'f' | 'F'] | ['f' | 'F', 'R'] => Self::RawFormat { uppercase_r: true },
            ['r', 'b' | 'B'] | ['b' | 'B', 'r'] => Self::RawBytes { uppercase_r: false },
            ['R', 'b' | 'B'] | ['b' | 'B', 'R'] => Self::RawBytes { uppercase_r: true },
            _ => return Err(format!("Unexpected prefix '{}{}'", value[0], value[1])),
        };
        Ok(result)
    }
}

impl StringPrefix {
    const fn as_flags(self) -> StringFlags {
        match self {
            Self::Bytes => StringFlags::B_PREFIX,
            Self::Format => StringFlags::F_PREFIX,
            Self::Raw { uppercase: true } => StringFlags::R_PREFIX_UPPER,
            Self::Raw { uppercase: false } => StringFlags::R_PREFIX_LOWER,
            Self::RawBytes { uppercase_r: true } => {
                StringFlags::R_PREFIX_UPPER.union(StringFlags::B_PREFIX)
            }
            Self::RawBytes { uppercase_r: false } => {
                StringFlags::R_PREFIX_LOWER.union(StringFlags::B_PREFIX)
            }
            Self::RawFormat { uppercase_r: true } => {
                StringFlags::R_PREFIX_UPPER.union(StringFlags::F_PREFIX)
            }
            Self::RawFormat { uppercase_r: false } => {
                StringFlags::R_PREFIX_LOWER.union(StringFlags::F_PREFIX)
            }
            Self::Unicode => StringFlags::U_PREFIX,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Bytes => "b",
            Self::Format => "f",
            Self::Unicode => "u",
            Self::Raw { uppercase: true } => "R",
            Self::Raw { uppercase: false } => "r",
            Self::RawBytes { uppercase_r: true } => "Rb",
            Self::RawBytes { uppercase_r: false } => "rb",
            Self::RawFormat { uppercase_r: true } => "Rf",
            Self::RawFormat { uppercase_r: false } => "rf",
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringKind(StringFlags);

impl StringKind {
    pub(crate) const fn from_prefix(prefix: Option<StringPrefix>) -> Self {
        if let Some(prefix) = prefix {
            Self(prefix.as_flags())
        } else {
            Self(StringFlags::empty())
        }
    }

    /// Does the string have a `u` or `U` prefix?
    pub const fn is_u_string(self) -> bool {
        self.0.contains(StringFlags::U_PREFIX)
    }

    /// Does the string have an `r` or `R` prefix?
    pub const fn is_raw_string(self) -> bool {
        self.0
            .intersects(StringFlags::R_PREFIX_LOWER.union(StringFlags::R_PREFIX_UPPER))
    }

    /// Does the string have an `f` or `F` prefix?
    pub const fn is_f_string(self) -> bool {
        self.0.contains(StringFlags::F_PREFIX)
    }

    /// Does the string have a `b` or `B` prefix?
    pub const fn is_byte_string(self) -> bool {
        self.0.contains(StringFlags::B_PREFIX)
    }

    /// Does the string use single or double quotes in its opener and closer?
    pub const fn quote_style(self) -> Quote {
        if self.0.contains(StringFlags::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    pub const fn is_triple_quoted(self) -> bool {
        self.0.contains(StringFlags::TRIPLE_QUOTED)
    }

    /// A `str` representation of the quotes used to start and close.
    /// This does not include any prefixes the string has in its opener.
    pub const fn quote_str(self) -> &'static str {
        if self.is_triple_quoted() {
            match self.quote_style() {
                Quote::Single => "'''",
                Quote::Double => r#"""""#,
            }
        } else {
            match self.quote_style() {
                Quote::Single => "'",
                Quote::Double => "\"",
            }
        }
    }

    const fn prefix(self) -> Option<StringPrefix> {
        if self.0.contains(StringFlags::F_PREFIX) {
            if self.0.contains(StringFlags::R_PREFIX_LOWER) {
                return Some(StringPrefix::RawFormat { uppercase_r: false });
            }
            if self.0.contains(StringFlags::R_PREFIX_UPPER) {
                return Some(StringPrefix::RawFormat { uppercase_r: true });
            }
            return Some(StringPrefix::Format);
        }
        if self.0.contains(StringFlags::B_PREFIX) {
            if self.0.contains(StringFlags::R_PREFIX_LOWER) {
                return Some(StringPrefix::RawBytes { uppercase_r: true });
            }
            if self.0.contains(StringFlags::R_PREFIX_LOWER) {
                return Some(StringPrefix::RawBytes { uppercase_r: false });
            }
            return Some(StringPrefix::Bytes);
        }
        if self.0.contains(StringFlags::R_PREFIX_LOWER) {
            return Some(StringPrefix::Raw { uppercase: false });
        }
        if self.0.contains(StringFlags::R_PREFIX_UPPER) {
            return Some(StringPrefix::Raw { uppercase: true });
        }
        if self.0.contains(StringFlags::U_PREFIX) {
            return Some(StringPrefix::Unicode);
        }
        None
    }

    /// A `str` representation of the prefixes used (if any)
    /// in the string's opener. The order of the prefixes is normalized,
    /// and all casing is normalized to lowercase except for `r` prefixes.
    ///
    /// See <https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings>
    /// for why we track the casing of the `r` prefix,
    /// but not for any other prefix.
    ///
    /// Examples:
    /// - `"foo"`       -> `""`
    /// - `B'foo'`      -> `"b"`
    /// - `"rf"{bar}"`  -> `"rf"`
    /// - `BR'{foo}'`   -> `"Rb"`
    pub const fn prefix_str(self) -> &'static str {
        if let Some(prefix) = self.prefix() {
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
        self.0 |= StringFlags::DOUBLE;
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self) -> Self {
        self.0 |= StringFlags::TRIPLE_QUOTED;
        self
    }
}

impl fmt::Debug for StringKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringKind")
            .field("prefix", &self.prefix_str())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("quote_style", &self.quote_style())
            .finish()
    }
}

impl From<StringKind> for ruff_python_ast::StringLiteralFlags {
    fn from(value: StringKind) -> ruff_python_ast::StringLiteralFlags {
        debug_assert!(!value.is_f_string());
        debug_assert!(!value.is_byte_string());

        let mut new = ruff_python_ast::StringLiteralFlags::default();
        if value.quote_style().is_double() {
            new = new.with_double_quotes();
        }
        if value.is_triple_quoted() {
            new = new.with_triple_quotes();
        }
        new.with_prefix({
            if value.is_u_string() {
                debug_assert!(!value.is_raw_string());
                StringLiteralPrefix::UString
            } else if value.is_raw_string() {
                StringLiteralPrefix::RString
            } else {
                StringLiteralPrefix::None
            }
        })
    }
}

impl From<StringKind> for ruff_python_ast::BytesLiteralFlags {
    fn from(value: StringKind) -> ruff_python_ast::BytesLiteralFlags {
        debug_assert!(value.is_byte_string());
        debug_assert!(!value.is_f_string());
        debug_assert!(!value.is_u_string());

        let mut new = ruff_python_ast::BytesLiteralFlags::default();
        if value.quote_style().is_double() {
            new = new.with_double_quotes();
        }
        if value.is_triple_quoted() {
            new = new.with_triple_quotes();
        }
        if value.is_raw_string() {
            new = new.with_r_prefix();
        }
        new
    }
}

impl From<StringKind> for ruff_python_ast::FStringFlags {
    fn from(value: StringKind) -> ruff_python_ast::FStringFlags {
        let mut new = ruff_python_ast::FStringFlags::default();
        if value.quote_style().is_double() {
            new = new.with_double_quotes();
        }
        if value.is_triple_quoted() {
            new = new.with_triple_quotes();
        }
        new.with_prefix(match value.prefix() {
            Some(StringPrefix::Format) => FStringPrefix::Regular,
            Some(StringPrefix::RawFormat { uppercase_r: false }) => {
                FStringPrefix::Raw { uppercase_r: false }
            }
            Some(StringPrefix::RawFormat { uppercase_r: true }) => {
                FStringPrefix::Raw { uppercase_r: true }
            }
            _ => panic!("Attempting to convert a non-f-string into an f-string!"),
        })
    }
}
