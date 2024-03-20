use std::fmt;

use bitflags::bitflags;

use ruff_python_ast::{str::Quote, ByteStringPrefix, FStringPrefix, StringLiteralPrefix};
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
pub enum StringPrefix {
    /// Prefixes that indicate the string is a bytestring
    Bytes(ByteStringPrefix),

    /// Prefixes that indicate the string is an f-string
    Format(FStringPrefix),

    /// All other prefixes
    Regular(StringLiteralPrefix),
}

impl TryFrom<char> for StringPrefix {
    type Error = String;

    fn try_from(value: char) -> Result<Self, String> {
        let result = match value {
            'r' => Self::Regular(StringLiteralPrefix::Raw { uppercase: false }),
            'R' => Self::Regular(StringLiteralPrefix::Raw { uppercase: true }),
            'u' | 'U' => Self::Regular(StringLiteralPrefix::Unicode),
            'b' | 'B' => Self::Bytes(ByteStringPrefix::Regular),
            'f' | 'F' => Self::Format(FStringPrefix::Regular),
            _ => return Err(format!("Unexpected prefix '{value}'")),
        };
        Ok(result)
    }
}

impl TryFrom<[char; 2]> for StringPrefix {
    type Error = String;

    fn try_from(value: [char; 2]) -> Result<Self, String> {
        let result = match value {
            ['r', 'f' | 'F'] | ['f' | 'F', 'r'] => {
                Self::Format(FStringPrefix::Raw { uppercase_r: false })
            }
            ['R', 'f' | 'F'] | ['f' | 'F', 'R'] => {
                Self::Format(FStringPrefix::Raw { uppercase_r: true })
            }
            ['r', 'b' | 'B'] | ['b' | 'B', 'r'] => {
                Self::Bytes(ByteStringPrefix::Raw { uppercase_r: false })
            }
            ['R', 'b' | 'B'] | ['b' | 'B', 'R'] => {
                Self::Bytes(ByteStringPrefix::Raw { uppercase_r: true })
            }
            _ => return Err(format!("Unexpected prefix '{}{}'", value[0], value[1])),
        };
        Ok(result)
    }
}

impl StringPrefix {
    const fn as_flags(self) -> StringFlags {
        match self {
            // regular strings
            Self::Regular(StringLiteralPrefix::Empty) => StringFlags::empty(),
            Self::Regular(StringLiteralPrefix::Unicode) => StringFlags::U_PREFIX,
            Self::Regular(StringLiteralPrefix::Raw { uppercase: false }) => {
                StringFlags::R_PREFIX_LOWER
            }
            Self::Regular(StringLiteralPrefix::Raw { uppercase: true }) => {
                StringFlags::R_PREFIX_UPPER
            }

            // bytestrings
            Self::Bytes(ByteStringPrefix::Regular) => StringFlags::B_PREFIX,
            Self::Bytes(ByteStringPrefix::Raw { uppercase_r: false }) => {
                StringFlags::B_PREFIX.union(StringFlags::R_PREFIX_LOWER)
            }
            Self::Bytes(ByteStringPrefix::Raw { uppercase_r: true }) => {
                StringFlags::B_PREFIX.union(StringFlags::R_PREFIX_UPPER)
            }

            // f-strings
            Self::Format(FStringPrefix::Regular) => StringFlags::F_PREFIX,
            Self::Format(FStringPrefix::Raw { uppercase_r: false }) => {
                StringFlags::F_PREFIX.union(StringFlags::R_PREFIX_LOWER)
            }
            Self::Format(FStringPrefix::Raw { uppercase_r: true }) => {
                StringFlags::F_PREFIX.union(StringFlags::R_PREFIX_UPPER)
            }
        }
    }

    const fn from_kind(kind: StringKind) -> Self {
        let StringKind(flags) = kind;

        // f-strings
        if flags.contains(StringFlags::F_PREFIX) {
            if flags.contains(StringFlags::R_PREFIX_LOWER) {
                return Self::Format(FStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(StringFlags::R_PREFIX_UPPER) {
                return Self::Format(FStringPrefix::Raw { uppercase_r: true });
            }
            return Self::Format(FStringPrefix::Regular);
        }

        // bytestrings
        if flags.contains(StringFlags::B_PREFIX) {
            if flags.contains(StringFlags::R_PREFIX_LOWER) {
                return Self::Bytes(ByteStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(StringFlags::R_PREFIX_UPPER) {
                return Self::Bytes(ByteStringPrefix::Raw { uppercase_r: true });
            }
            return Self::Bytes(ByteStringPrefix::Regular);
        }

        // all other strings
        if flags.contains(StringFlags::R_PREFIX_LOWER) {
            return Self::Regular(StringLiteralPrefix::Raw { uppercase: false });
        }
        if flags.contains(StringFlags::R_PREFIX_UPPER) {
            return Self::Regular(StringLiteralPrefix::Raw { uppercase: true });
        }
        if flags.contains(StringFlags::U_PREFIX) {
            return Self::Regular(StringLiteralPrefix::Unicode);
        }
        Self::Regular(StringLiteralPrefix::Empty)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular(regular_prefix) => regular_prefix.as_str(),
            Self::Bytes(bytestring_prefix) => bytestring_prefix.as_str(),
            Self::Format(fstring_prefix) => fstring_prefix.as_str(),
        }
    }
}

impl fmt::Display for StringPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for StringPrefix {
    fn default() -> Self {
        Self::Regular(StringLiteralPrefix::Empty)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringKind(StringFlags);

impl StringKind {
    #[must_use]
    pub fn with_prefix(mut self, prefix: StringPrefix) -> Self {
        self.0 |= prefix.as_flags();
        self
    }

    pub const fn prefix(self) -> StringPrefix {
        StringPrefix::from_kind(self)
    }

    pub fn new(prefix: StringPrefix, quotes: Quote, triple_quoted: bool) -> Self {
        let new = Self::default().with_prefix(prefix).with_quote_style(quotes);
        if triple_quoted {
            new.with_triple_quotes()
        } else {
            new
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

    /// The length of the prefixes used (if any) in the string's opener.
    pub fn prefix_len(self) -> TextSize {
        self.prefix().as_str().text_len()
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
            self.prefix(),
            self.quote_str(),
            contents,
            self.quote_str()
        )
    }

    #[must_use]
    pub fn with_quote_style(mut self, quotes: Quote) -> Self {
        match quotes {
            Quote::Double => self.0 |= StringFlags::DOUBLE,
            Quote::Single => self.0 -= StringFlags::DOUBLE,
        };
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
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("quote_style", &self.quote_style())
            .finish()
    }
}

impl From<StringKind> for ruff_python_ast::StringLiteralFlags {
    fn from(value: StringKind) -> ruff_python_ast::StringLiteralFlags {
        let StringPrefix::Regular(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a regular string",
                value.prefix()
            )
        };
        let new = ruff_python_ast::StringLiteralFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<ruff_python_ast::StringLiteralFlags> for StringKind {
    fn from(value: ruff_python_ast::StringLiteralFlags) -> Self {
        Self::new(
            StringPrefix::Regular(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
    }
}

impl From<StringKind> for ruff_python_ast::BytesLiteralFlags {
    fn from(value: StringKind) -> ruff_python_ast::BytesLiteralFlags {
        let StringPrefix::Bytes(bytestring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a bytestring",
                value.prefix()
            )
        };
        let new = ruff_python_ast::BytesLiteralFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(bytestring_prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<ruff_python_ast::BytesLiteralFlags> for StringKind {
    fn from(value: ruff_python_ast::BytesLiteralFlags) -> Self {
        Self::new(
            StringPrefix::Bytes(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
    }
}

impl From<StringKind> for ruff_python_ast::FStringFlags {
    fn from(value: StringKind) -> ruff_python_ast::FStringFlags {
        let StringPrefix::Format(fstring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into an f-string",
                value.prefix()
            )
        };
        let new = ruff_python_ast::FStringFlags::default()
            .with_quote_style(value.quote_style())
            .with_prefix(fstring_prefix);
        if value.is_triple_quoted() {
            new.with_triple_quotes()
        } else {
            new
        }
    }
}

impl From<ruff_python_ast::FStringFlags> for StringKind {
    fn from(value: ruff_python_ast::FStringFlags) -> Self {
        Self::new(
            StringPrefix::Format(value.prefix()),
            value.quote_style(),
            value.is_triple_quoted(),
        )
    }
}
