use std::fmt;

/// Enumerations of the valid prefixes a string literal can have.
///
/// Bytestrings and f-strings are excluded from this enumeration,
/// as they are represented by different AST nodes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, is_macro::Is)]
pub enum StringLiteralPrefix {
    /// Just a regular string with no prefixes
    Empty,

    /// A string with a `u` or `U` prefix, e.g. `u"foo"`.
    /// Note that, despite this variant's name,
    /// it is in fact a no-op at runtime to use the `u` or `U` prefix
    /// in Python. All Python-3 strings are unicode strings;
    /// this prefix is only allowed in Python 3 for backwards compatibility
    /// with Python 2. However, using this prefix in a Python string
    /// is mutually exclusive with an `r` or `R` prefix.
    Unicode,

    /// A "raw" string, that has an `r` or `R` prefix,
    /// e.g. `r"foo\."` or `R'bar\d'`.
    Raw { uppercase: bool },
}

impl StringLiteralPrefix {
    /// Return a `str` representation of the prefix
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Unicode => "u",
            Self::Raw { uppercase: true } => "R",
            Self::Raw { uppercase: false } => "r",
        }
    }
}

impl fmt::Display for StringLiteralPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Enumeration of the valid prefixes an f-string literal can have.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FStringPrefix {
    /// Just a regular f-string with no other prefixes, e.g. f"{bar}"
    Regular,

    /// A "raw" format-string, that has an `r` or `R` prefix,
    /// e.g. `rf"{bar}"` or `Rf"{bar}"`
    Raw { uppercase_r: bool },
}

impl FStringPrefix {
    /// Return a `str` representation of the prefix
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "f",
            Self::Raw { uppercase_r: true } => "Rf",
            Self::Raw { uppercase_r: false } => "rf",
        }
    }

    /// Return true if this prefix indicates a "raw f-string",
    /// e.g. `rf"{bar}"` or `Rf"{bar}"`
    pub const fn is_raw(self) -> bool {
        matches!(self, Self::Raw { .. })
    }
}

impl fmt::Display for FStringPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Enumeration of the valid prefixes a bytestring literal can have.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ByteStringPrefix {
    /// Just a regular bytestring with no other prefixes, e.g. `b"foo"`
    Regular,

    /// A "raw" bytestring, that has an `r` or `R` prefix,
    /// e.g. `Rb"foo"` or `rb"foo"`
    Raw { uppercase_r: bool },
}

impl ByteStringPrefix {
    /// Return a `str` representation of the prefix
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "b",
            Self::Raw { uppercase_r: true } => "Rb",
            Self::Raw { uppercase_r: false } => "rb",
        }
    }

    /// Return true if this prefix indicates a "raw bytestring",
    /// e.g. `rb"foo"` or `Rb"foo"`
    pub const fn is_raw(self) -> bool {
        matches!(self, Self::Raw { .. })
    }
}

impl fmt::Display for ByteStringPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, is_macro::Is)]
pub enum AnyStringPrefix {
    /// Prefixes that indicate the string is a bytestring
    Bytes(ByteStringPrefix),

    /// Prefixes that indicate the string is an f-string
    Format(FStringPrefix),

    /// All other prefixes
    Regular(StringLiteralPrefix),
}

impl AnyStringPrefix {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular(regular_prefix) => regular_prefix.as_str(),
            Self::Bytes(bytestring_prefix) => bytestring_prefix.as_str(),
            Self::Format(fstring_prefix) => fstring_prefix.as_str(),
        }
    }

    pub const fn is_raw(self) -> bool {
        match self {
            Self::Regular(regular_prefix) => regular_prefix.is_raw(),
            Self::Bytes(bytestring_prefix) => bytestring_prefix.is_raw(),
            Self::Format(fstring_prefix) => fstring_prefix.is_raw(),
        }
    }
}

impl fmt::Display for AnyStringPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for AnyStringPrefix {
    fn default() -> Self {
        Self::Regular(StringLiteralPrefix::Empty)
    }
}
