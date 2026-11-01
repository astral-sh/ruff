use std::fmt;

use bitflags::bitflags;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::str::{Quote, TripleQuotes};
use crate::str_prefix::{
    AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix, TStringPrefix,
};
use crate::{NodeIndex, int, name::Name};

/// A literal or formatted part of an implicitly concatenated f-string.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FStringPart {}

/// A key and its pattern, or the `**name` capture in a mapping pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum MappingPatternItem<K, P, R> {
    Entry { key: K, pattern: P },
    Rest(R),
}

/// Transforms a value prior to formatting it.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
#[repr(i8)]
#[expect(clippy::cast_possible_wrap)]
pub enum ConversionFlag {
    /// No conversion
    None = -1, // CPython uses -1
    /// Converts by calling `str(<value>)`.
    Str = b's' as i8,
    /// Converts by calling `ascii(<value>)`.
    Ascii = b'a' as i8,
    /// Converts by calling `repr(<value>)`.
    Repr = b'r' as i8,
}

impl ConversionFlag {
    pub fn to_byte(&self) -> Option<u8> {
        match self {
            Self::None => None,
            flag => Some(*flag as u8),
        }
    }
    pub fn to_char(&self) -> Option<char> {
        Some(self.to_byte()? as char)
    }
}

/// The debug text of a self-documenting f-string expression (e.g., `f"{x=}"`).
///
/// Stores the concatenation of leading text, expression source, and trailing text as a single
/// [`CompactString`], with byte offsets to split them. The offsets are needed because the leading
/// and trailing portions can contain non-whitespace characters (grouping parentheses, comments in
/// triple-quoted f-strings) that cannot be distinguished from expression content by scanning.
///
/// [`CompactString`]: compact_str::CompactString
#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct DebugText {
    /// The full text between the `{` and the conversion / `format_spec` / `}`.
    text: compact_str::CompactString,
    /// Byte offset where the expression source begins.
    expression_start: u32,
    /// Byte offset where the expression source ends.
    expression_end: u32,
}

impl std::fmt::Debug for DebugText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugText")
            .field("leading", &self.leading())
            .field("expression", &self.expression())
            .field("trailing", &self.trailing())
            .finish()
    }
}

impl DebugText {
    pub fn new(leading: &str, expression: &str, trailing: &str) -> Self {
        let expression_start = leading.text_len().to_u32();
        let expression_end = expression_start + expression.text_len().to_u32();
        let mut buf = compact_str::CompactString::with_capacity(
            leading.len() + expression.len() + trailing.len(),
        );
        buf.push_str(leading);
        buf.push_str(expression);
        buf.push_str(trailing);
        Self {
            text: buf,
            expression_start,
            expression_end,
        }
    }

    /// The full debug text between the `{` and the conversion / `format_spec` / `}`.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The text between the `{` and the expression node.
    pub fn leading(&self) -> &str {
        &self.text[..self.expression_start as usize]
    }

    /// The source text of the expression (e.g., `0x0` in `f"{0x0=}"`).
    pub fn expression(&self) -> &str {
        &self.text[self.expression_start as usize..self.expression_end as usize]
    }

    /// The text between the expression and the conversion, the `format_spec`, or the `}`.
    pub fn trailing(&self) -> &str {
        &self.text[self.expression_end as usize..]
    }
}

pub trait StringFlags: Copy {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote;

    fn triple_quotes(self) -> TripleQuotes;

    fn prefix(self) -> AnyStringPrefix;

    fn is_unclosed(self) -> bool;

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    fn is_triple_quoted(self) -> bool {
        self.triple_quotes().is_yes()
    }

    /// A `str` representation of the quotes used to start and close.
    /// This does not include any prefixes the string has in its opener.
    fn quote_str(self) -> &'static str {
        match (self.triple_quotes(), self.quote_style()) {
            (TripleQuotes::Yes, Quote::Single) => "'''",
            (TripleQuotes::Yes, Quote::Double) => r#"""""#,
            (TripleQuotes::No, Quote::Single) => "'",
            (TripleQuotes::No, Quote::Double) => "\"",
        }
    }

    /// The length of the quotes used to start and close the string.
    /// This does not include the length of any prefixes the string has
    /// in its opener.
    fn quote_len(self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }

    /// The total length of the string's opener,
    /// i.e., the length of the prefixes plus the length
    /// of the quotes used to open the string.
    fn opener_len(self) -> TextSize {
        self.prefix().text_len() + self.quote_len()
    }

    /// The total length of the string's closer.
    /// This is always equal to `self.quote_len()`, except when the string is unclosed,
    /// in which case the length is zero.
    fn closer_len(self) -> TextSize {
        if self.is_unclosed() {
            TextSize::default()
        } else {
            self.quote_len()
        }
    }

    fn as_any_string_flags(self) -> AnyStringFlags {
        AnyStringFlags::new(self.prefix(), self.quote_style(), self.triple_quotes())
            .with_unclosed(self.is_unclosed())
    }

    fn display_contents(self, contents: &str) -> DisplayFlags<'_> {
        DisplayFlags {
            flags: self.as_any_string_flags(),
            contents,
        }
    }
}

pub struct DisplayFlags<'a> {
    flags: AnyStringFlags,
    contents: &'a str,
}

impl std::fmt::Display for DisplayFlags<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{prefix}{quote}{contents}{quote}",
            prefix = self.flags.prefix(),
            quote = self.flags.quote_str(),
            contents = self.contents
        )
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct InterpolatedStringFlagsInner: u8 {
        /// The f-string uses double quotes (`"`) for its opener and closer.
        /// If this flag is not set, the f-string uses single quotes (`'`)
        /// for its opener and closer.
        const DOUBLE = 1 << 0;

        /// The f-string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        /// For example: `f"""{bar}"""`.
        const TRIPLE_QUOTED = 1 << 1;

        /// The f-string has an `r` prefix, meaning it is a raw f-string
        /// with a lowercase 'r'. For example: `rf"{bar}"`
        const R_PREFIX_LOWER = 1 << 2;

        /// The f-string has an `R` prefix, meaning it is a raw f-string
        /// with an uppercase 'r'. For example: `Rf"{bar}"`.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;

        /// The f-string is unclosed, meaning it is missing a closing quote.
        /// For example: `f"{bar`
        const UNCLOSED = 1 << 4;
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for InterpolatedStringFlagsInner {}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for an f-string.
///
/// Note: This is identical to [`TStringFlags`] except that
/// the implementation of the `prefix` method of the
/// [`StringFlags`] trait returns a variant of
/// `AnyStringPrefix::Format`.
///
/// ## Notes on usage
///
/// A lint fix can preserve an existing f-string's prefix and quotes by copying its
/// `flags()`. For a new f-string, `Checker::default_fstring_flags` chooses quotes
/// appropriate to the surrounding syntax. Outside the linter, start with [`FStringFlags::empty`].
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct FStringFlags(InterpolatedStringFlagsInner);

impl FStringFlags {
    /// Construct a new [`FStringFlags`] with **no flags set**.
    ///
    /// See [`FStringFlags::with_quote_style`], [`FStringFlags::with_triple_quotes`], and
    /// [`FStringFlags::with_prefix`] for ways of setting the quote style (single or double),
    /// enabling triple quotes, and adding prefixes (such as `r`), respectively.
    ///
    /// See the documentation for [`FStringFlags`] for additional caveats on this constructor, and
    /// situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(InterpolatedStringFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0.set(
            InterpolatedStringFlagsInner::DOUBLE,
            quote_style.is_double(),
        );
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            InterpolatedStringFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_unclosed(mut self, unclosed: bool) -> Self {
        self.0.set(InterpolatedStringFlagsInner::UNCLOSED, unclosed);
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: FStringPrefix) -> Self {
        match prefix {
            FStringPrefix::Regular => Self(
                self.0
                    - InterpolatedStringFlagsInner::R_PREFIX_LOWER
                    - InterpolatedStringFlagsInner::R_PREFIX_UPPER,
            ),
            FStringPrefix::Raw { uppercase_r } => {
                self.0
                    .set(InterpolatedStringFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0
                    .set(InterpolatedStringFlagsInner::R_PREFIX_LOWER, !uppercase_r);
                self
            }
        }
    }

    pub const fn prefix(self) -> FStringPrefix {
        if self
            .0
            .contains(InterpolatedStringFlagsInner::R_PREFIX_LOWER)
        {
            debug_assert!(
                !self
                    .0
                    .contains(InterpolatedStringFlagsInner::R_PREFIX_UPPER)
            );
            FStringPrefix::Raw { uppercase_r: false }
        } else if self
            .0
            .contains(InterpolatedStringFlagsInner::R_PREFIX_UPPER)
        {
            FStringPrefix::Raw { uppercase_r: true }
        } else {
            FStringPrefix::Regular
        }
    }
}

// TODO(dylan): the documentation about using
// `Checker::default_tstring_flags` is not yet
// correct. This method does not yet exist because
// introducing it would emit a dead code warning
// until we call it in lint rules.
/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for an f-string.
///
/// Note: This is identical to [`FStringFlags`] except that
/// the implementation of the `prefix` method of the
/// [`StringFlags`] trait returns a variant of
/// `AnyStringPrefix::Template`.
///
/// ## Notes on usage
///
/// A replacement t-string can preserve the prefix and quotes of a parsed literal by
/// copying its `flags()`. For a new t-string, start with [`TStringFlags::empty`].
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct TStringFlags(InterpolatedStringFlagsInner);

impl TStringFlags {
    /// Construct a new [`TStringFlags`] with **no flags set**.
    ///
    /// See [`TStringFlags::with_quote_style`], [`TStringFlags::with_triple_quotes`], and
    /// [`TStringFlags::with_prefix`] for ways of setting the quote style (single or double),
    /// enabling triple quotes, and adding prefixes (such as `r`), respectively.
    ///
    /// See the documentation for [`TStringFlags`] for additional caveats on this constructor, and
    /// situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(InterpolatedStringFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0.set(
            InterpolatedStringFlagsInner::DOUBLE,
            quote_style.is_double(),
        );
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            InterpolatedStringFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_unclosed(mut self, unclosed: bool) -> Self {
        self.0.set(InterpolatedStringFlagsInner::UNCLOSED, unclosed);
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: TStringPrefix) -> Self {
        match prefix {
            TStringPrefix::Regular => Self(
                self.0
                    - InterpolatedStringFlagsInner::R_PREFIX_LOWER
                    - InterpolatedStringFlagsInner::R_PREFIX_UPPER,
            ),
            TStringPrefix::Raw { uppercase_r } => {
                self.0
                    .set(InterpolatedStringFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0
                    .set(InterpolatedStringFlagsInner::R_PREFIX_LOWER, !uppercase_r);
                self
            }
        }
    }

    pub const fn prefix(self) -> TStringPrefix {
        if self
            .0
            .contains(InterpolatedStringFlagsInner::R_PREFIX_LOWER)
        {
            debug_assert!(
                !self
                    .0
                    .contains(InterpolatedStringFlagsInner::R_PREFIX_UPPER)
            );
            TStringPrefix::Raw { uppercase_r: false }
        } else if self
            .0
            .contains(InterpolatedStringFlagsInner::R_PREFIX_UPPER)
        {
            TStringPrefix::Raw { uppercase_r: true }
        } else {
            TStringPrefix::Regular
        }
    }
}

impl StringFlags for FStringFlags {
    /// Return `true` if the f-string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `f"""{bar}"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(InterpolatedStringFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    /// Return the quoting style (single or double quotes)
    /// used by the f-string's opener and closer:
    /// - `f"{"a"}"` -> `QuoteStyle::Double`
    /// - `f'{"a"}'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(InterpolatedStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Format(self.prefix())
    }

    fn is_unclosed(self) -> bool {
        self.0.intersects(InterpolatedStringFlagsInner::UNCLOSED)
    }
}

impl fmt::Debug for FStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FStringFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("unclosed", &self.is_unclosed())
            .finish()
    }
}

impl StringFlags for TStringFlags {
    /// Return `true` if the t-string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `t"""{bar}"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(InterpolatedStringFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    /// Return the quoting style (single or double quotes)
    /// used by the t-string's opener and closer:
    /// - `t"{"a"}"` -> `QuoteStyle::Double`
    /// - `t'{"a"}'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(InterpolatedStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Template(self.prefix())
    }

    fn is_unclosed(self) -> bool {
        self.0.intersects(InterpolatedStringFlagsInner::UNCLOSED)
    }
}

impl fmt::Debug for TStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TStringFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("unclosed", &self.is_unclosed())
            .finish()
    }
}

bitflags! {
    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct StringLiteralFlagsInner: u8 {
        /// The string uses double quotes (e.g. `"foo"`).
        /// If this flag is not set, the string uses single quotes (`'foo'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted (`"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix, e.g. `u"foo"`.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set;
        /// it is therefore invalid for this flag to be set
        /// if `R_PREFIX` is also set.
        const U_PREFIX = 1 << 2;

        /// The string has an `r` prefix, meaning it is a raw string
        /// with a lowercase 'r' (e.g. `r"foo\."`).
        /// It is invalid to set this flag if `U_PREFIX` is also set.
        const R_PREFIX_LOWER = 1 << 3;

        /// The string has an `R` prefix, meaning it is a raw string
        /// with an uppercase 'R' (e.g. `R'foo\d'`).
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 4;

        /// The string was deemed invalid by the parser.
        const INVALID = 1 << 5;

        /// The string literal misses the matching closing quote(s).
        const UNCLOSED = 1 << 6;
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for StringLiteralFlagsInner {}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a string literal.
///
/// ## Notes on usage
///
/// To preserve the prefix and quotes of a parsed string, copy the literal's `flags()`.
/// For implicit concatenation, [`StringValue::first_literal_flags`](crate::flat::StringValue::first_literal_flags)
/// returns the flags of the first literal. For a new string, `Checker::default_string_flags`
/// chooses quotes appropriate to the surrounding syntax. Outside the linter, start with
/// [`StringLiteralFlags::empty`].
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StringLiteralFlags(StringLiteralFlagsInner);

impl StringLiteralFlags {
    /// Construct a new [`StringLiteralFlags`] with **no flags set**.
    ///
    /// See [`StringLiteralFlags::with_quote_style`], [`StringLiteralFlags::with_triple_quotes`],
    /// and [`StringLiteralFlags::with_prefix`] for ways of setting the quote style (single or
    /// double), enabling triple quotes, and adding prefixes (such as `r` or `u`), respectively.
    ///
    /// See the documentation for [`StringLiteralFlags`] for additional caveats on this constructor,
    /// and situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(StringLiteralFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(StringLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            StringLiteralFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_unclosed(mut self, unclosed: bool) -> Self {
        self.0.set(StringLiteralFlagsInner::UNCLOSED, unclosed);
        self
    }

    #[must_use]
    pub fn with_prefix(self, prefix: StringLiteralPrefix) -> Self {
        let StringLiteralFlags(flags) = self;
        match prefix {
            StringLiteralPrefix::Empty => Self(
                flags
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: false } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_LOWER)
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: true } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_UPPER)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Unicode => Self(
                (flags | StringLiteralFlagsInner::U_PREFIX)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER,
            ),
        }
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= StringLiteralFlagsInner::INVALID;
        self
    }

    /// Returns `true` if the parser deemed the string literal invalid.
    pub const fn is_invalid(self) -> bool {
        self.0.contains(StringLiteralFlagsInner::INVALID)
    }

    pub const fn prefix(self) -> StringLiteralPrefix {
        if self.0.contains(StringLiteralFlagsInner::U_PREFIX) {
            debug_assert!(
                !self.0.intersects(
                    StringLiteralFlagsInner::R_PREFIX_LOWER
                        .union(StringLiteralFlagsInner::R_PREFIX_UPPER)
                )
            );
            StringLiteralPrefix::Unicode
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER));
            StringLiteralPrefix::Raw { uppercase: false }
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER) {
            StringLiteralPrefix::Raw { uppercase: true }
        } else {
            StringLiteralPrefix::Empty
        }
    }
}

impl StringFlags for StringLiteralFlags {
    /// Return the quoting style (single or double quotes)
    /// used by the string's opener and closer:
    /// - `"a"` -> `QuoteStyle::Double`
    /// - `'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(StringLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    /// Return `true` if the string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `"""bar"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(StringLiteralFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Regular(self.prefix())
    }

    fn is_unclosed(self) -> bool {
        self.0.intersects(StringLiteralFlagsInner::UNCLOSED)
    }
}

impl fmt::Debug for StringLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("unclosed", &self.is_unclosed())
            .finish()
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct BytesLiteralFlagsInner: u8 {
        /// The bytestring uses double quotes (e.g. `b"foo"`).
        /// If this flag is not set, the bytestring uses single quotes (e.g. `b'foo'`).
        const DOUBLE = 1 << 0;

        /// The bytestring is triple-quoted (e.g. `b"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The bytestring has an `r` prefix (e.g. `rb"foo"`),
        /// meaning it is a raw bytestring with a lowercase 'r'.
        const R_PREFIX_LOWER = 1 << 2;

        /// The bytestring has an `R` prefix (e.g. `Rb"foo"`),
        /// meaning it is a raw bytestring with an uppercase 'R'.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix, but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;

        /// The bytestring was deemed invalid by the parser.
        const INVALID = 1 << 4;

        /// The byte string misses the matching closing quote(s).
        const UNCLOSED = 1 << 5;
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for BytesLiteralFlagsInner {}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a bytes literal.
///
/// ## Notes on usage
///
/// To preserve the prefix and quotes of a parsed bytes literal, copy its `flags()`.
/// For a new literal, `Checker::default_bytes_flags` chooses quotes appropriate to the
/// surrounding syntax. Outside the linter, start with [`BytesLiteralFlags::empty`].
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct BytesLiteralFlags(BytesLiteralFlagsInner);

macro_rules! string_flag_bits {
    ($flags:ty, $bits:ty) => {
        impl $flags {
            pub(crate) const fn bits(self) -> u8 {
                self.0.bits()
            }

            pub(crate) const fn from_bits(bits: u8) -> Self {
                Self(<$bits>::from_bits_retain(bits))
            }
        }
    };
}

string_flag_bits!(StringLiteralFlags, StringLiteralFlagsInner);
string_flag_bits!(BytesLiteralFlags, BytesLiteralFlagsInner);
string_flag_bits!(FStringFlags, InterpolatedStringFlagsInner);
string_flag_bits!(TStringFlags, InterpolatedStringFlagsInner);

impl BytesLiteralFlags {
    /// Construct a new [`BytesLiteralFlags`] with **no flags set**.
    ///
    /// See [`BytesLiteralFlags::with_quote_style`], [`BytesLiteralFlags::with_triple_quotes`], and
    /// [`BytesLiteralFlags::with_prefix`] for ways of setting the quote style (single or double),
    /// enabling triple quotes, and adding prefixes (such as `r`), respectively.
    ///
    /// See the documentation for [`BytesLiteralFlags`] for additional caveats on this constructor,
    /// and situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(BytesLiteralFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(BytesLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            BytesLiteralFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_unclosed(mut self, unclosed: bool) -> Self {
        self.0.set(BytesLiteralFlagsInner::UNCLOSED, unclosed);
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: ByteStringPrefix) -> Self {
        match prefix {
            ByteStringPrefix::Regular => {
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_LOWER;
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_UPPER;
            }
            ByteStringPrefix::Raw { uppercase_r } => {
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_LOWER, !uppercase_r);
            }
        }
        self
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= BytesLiteralFlagsInner::INVALID;
        self
    }

    /// Returns `true` if the parser deemed the bytes literal invalid.
    pub const fn is_invalid(self) -> bool {
        self.0.contains(BytesLiteralFlagsInner::INVALID)
    }

    pub const fn prefix(self) -> ByteStringPrefix {
        if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER));
            ByteStringPrefix::Raw { uppercase_r: false }
        } else if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER) {
            ByteStringPrefix::Raw { uppercase_r: true }
        } else {
            ByteStringPrefix::Regular
        }
    }
}

impl StringFlags for BytesLiteralFlags {
    /// Return `true` if the bytestring is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `b"""{bar}"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(BytesLiteralFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    /// Return the quoting style (single or double quotes)
    /// used by the bytestring's opener and closer:
    /// - `b"a"` -> `QuoteStyle::Double`
    /// - `b'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(BytesLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Bytes(self.prefix())
    }

    fn is_unclosed(self) -> bool {
        self.0.intersects(BytesLiteralFlagsInner::UNCLOSED)
    }
}

impl fmt::Debug for BytesLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BytesLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("unclosed", &self.is_unclosed())
            .finish()
    }
}

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
    struct AnyStringFlagsInner: u16 {
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

        /// The string has a `t` or `T` prefix, meaning it is a t-string.
        /// T-strings can also be raw strings,
        /// but can have no other prefixes.
        const T_PREFIX = 1 << 5;

        /// The string has an `r` prefix, meaning it is a raw string.
        /// F-strings and byte-strings can be raw,
        /// as can strings with no other prefixes.
        /// U-strings cannot be raw.
        const R_PREFIX_LOWER = 1 << 6;

        /// The string has an `R` prefix, meaning it is a raw string.
        /// The casing of the `r`/`R` has no semantic significance at runtime;
        /// see https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 7;

        /// String without matching closing quote(s).
        const UNCLOSED = 1 << 8;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyStringFlags(AnyStringFlagsInner);

impl AnyStringFlags {
    #[must_use]
    pub fn with_prefix(mut self, prefix: AnyStringPrefix) -> Self {
        self.0 |= match prefix {
            // regular strings
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty) => AnyStringFlagsInner::empty(),
            AnyStringPrefix::Regular(StringLiteralPrefix::Unicode) => AnyStringFlagsInner::U_PREFIX,
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false }) => {
                AnyStringFlagsInner::R_PREFIX_LOWER
            }
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true }) => {
                AnyStringFlagsInner::R_PREFIX_UPPER
            }

            // bytestrings
            AnyStringPrefix::Bytes(ByteStringPrefix::Regular) => AnyStringFlagsInner::B_PREFIX,
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }

            // f-strings
            AnyStringPrefix::Format(FStringPrefix::Regular) => AnyStringFlagsInner::F_PREFIX,
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }

            // t-strings
            AnyStringPrefix::Template(TStringPrefix::Regular) => AnyStringFlagsInner::T_PREFIX,
            AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::T_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::T_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }
        };
        self
    }

    pub fn new(prefix: AnyStringPrefix, quotes: Quote, triple_quotes: TripleQuotes) -> Self {
        Self(AnyStringFlagsInner::empty())
            .with_prefix(prefix)
            .with_quote_style(quotes)
            .with_triple_quotes(triple_quotes)
    }

    /// Does the string have a `u` or `U` prefix?
    pub const fn is_u_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::U_PREFIX)
    }

    /// Does the string have an `r` or `R` prefix?
    pub const fn is_raw_string(self) -> bool {
        self.0.intersects(
            AnyStringFlagsInner::R_PREFIX_LOWER.union(AnyStringFlagsInner::R_PREFIX_UPPER),
        )
    }

    /// Does the string have an `f`,`F`,`t`, or `T` prefix?
    pub const fn is_interpolated_string(self) -> bool {
        self.0
            .intersects(AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::T_PREFIX))
    }

    /// Does the string have a `b` or `B` prefix?
    pub const fn is_byte_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::B_PREFIX)
    }

    #[must_use]
    pub fn with_quote_style(mut self, quotes: Quote) -> Self {
        match quotes {
            Quote::Double => self.0 |= AnyStringFlagsInner::DOUBLE,
            Quote::Single => self.0 -= AnyStringFlagsInner::DOUBLE,
        }
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0
            .set(AnyStringFlagsInner::TRIPLE_QUOTED, triple_quotes.is_yes());
        self
    }

    #[must_use]
    pub fn with_unclosed(mut self, unclosed: bool) -> Self {
        self.0.set(AnyStringFlagsInner::UNCLOSED, unclosed);
        self
    }
}

impl StringFlags for AnyStringFlags {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote {
        if self.0.contains(AnyStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(AnyStringFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        let AnyStringFlags(flags) = self;

        // f-strings
        if flags.contains(AnyStringFlagsInner::F_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Format(FStringPrefix::Regular);
        }

        // t-strings
        if flags.contains(AnyStringFlagsInner::T_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Template(TStringPrefix::Regular);
        }

        // bytestrings
        if flags.contains(AnyStringFlagsInner::B_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Bytes(ByteStringPrefix::Regular);
        }

        // all other strings
        if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false });
        }
        if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true });
        }
        if flags.contains(AnyStringFlagsInner::U_PREFIX) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Unicode);
        }
        AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
    }

    fn is_unclosed(self) -> bool {
        self.0.intersects(AnyStringFlagsInner::UNCLOSED)
    }
}

impl fmt::Debug for AnyStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyStringFlags")
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("quote_style", &self.quote_style())
            .field("unclosed", &self.is_unclosed())
            .finish()
    }
}

impl From<AnyStringFlags> for StringLiteralFlags {
    fn from(value: AnyStringFlags) -> StringLiteralFlags {
        let AnyStringPrefix::Regular(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a regular string",
                value.prefix()
            )
        };
        StringLiteralFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix)
            .with_triple_quotes(value.triple_quotes())
            .with_unclosed(value.is_unclosed())
    }
}

impl From<StringLiteralFlags> for AnyStringFlags {
    fn from(value: StringLiteralFlags) -> Self {
        value.as_any_string_flags()
    }
}

impl From<AnyStringFlags> for BytesLiteralFlags {
    fn from(value: AnyStringFlags) -> BytesLiteralFlags {
        let AnyStringPrefix::Bytes(bytestring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a bytestring",
                value.prefix()
            )
        };
        BytesLiteralFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(bytestring_prefix)
            .with_triple_quotes(value.triple_quotes())
            .with_unclosed(value.is_unclosed())
    }
}

impl From<BytesLiteralFlags> for AnyStringFlags {
    fn from(value: BytesLiteralFlags) -> Self {
        value.as_any_string_flags()
    }
}

impl From<AnyStringFlags> for FStringFlags {
    fn from(value: AnyStringFlags) -> FStringFlags {
        let AnyStringPrefix::Format(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into an f-string",
                value.prefix()
            )
        };
        FStringFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix)
            .with_triple_quotes(value.triple_quotes())
            .with_unclosed(value.is_unclosed())
    }
}

impl From<FStringFlags> for AnyStringFlags {
    fn from(value: FStringFlags) -> Self {
        value.as_any_string_flags()
    }
}

impl From<AnyStringFlags> for TStringFlags {
    fn from(value: AnyStringFlags) -> TStringFlags {
        let AnyStringPrefix::Template(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a t-string",
                value.prefix()
            )
        };
        TStringFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix)
            .with_triple_quotes(value.triple_quotes())
            .with_unclosed(value.is_unclosed())
    }
}

impl From<TStringFlags> for AnyStringFlags {
    fn from(value: TStringFlags) -> Self {
        value.as_any_string_flags()
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Number {
    Int(int::Int),
    Float(f64),
    Complex { real: f64, imag: f64 },
}

/// See also [expr_context](https://docs.python.org/3/library/ast.html#ast.expr_context)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum ExprContext {
    Load,
    Store,
    Del,
    Invalid,
}

/// See also [boolop](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum BoolOp {
    And,
    Or,
}

impl BoolOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        }
    }
}

impl fmt::Display for BoolOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [operator](https://docs.python.org/3/library/ast.html#ast.operator)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Operator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}

impl Operator {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mult => "*",
            Operator::MatMult => "@",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::Pow => "**",
            Operator::LShift => "<<",
            Operator::RShift => ">>",
            Operator::BitOr => "|",
            Operator::BitXor => "^",
            Operator::BitAnd => "&",
            Operator::FloorDiv => "//",
        }
    }

    /// Returns the dunder method name for the operator.
    pub const fn dunder(self) -> &'static str {
        match self {
            Operator::Add => "__add__",
            Operator::Sub => "__sub__",
            Operator::Mult => "__mul__",
            Operator::MatMult => "__matmul__",
            Operator::Div => "__truediv__",
            Operator::Mod => "__mod__",
            Operator::Pow => "__pow__",
            Operator::LShift => "__lshift__",
            Operator::RShift => "__rshift__",
            Operator::BitOr => "__or__",
            Operator::BitXor => "__xor__",
            Operator::BitAnd => "__and__",
            Operator::FloorDiv => "__floordiv__",
        }
    }

    /// Returns the in-place dunder method name for the operator.
    pub const fn in_place_dunder(self) -> &'static str {
        match self {
            Operator::Add => "__iadd__",
            Operator::Sub => "__isub__",
            Operator::Mult => "__imul__",
            Operator::MatMult => "__imatmul__",
            Operator::Div => "__itruediv__",
            Operator::Mod => "__imod__",
            Operator::Pow => "__ipow__",
            Operator::LShift => "__ilshift__",
            Operator::RShift => "__irshift__",
            Operator::BitOr => "__ior__",
            Operator::BitXor => "__ixor__",
            Operator::BitAnd => "__iand__",
            Operator::FloorDiv => "__ifloordiv__",
        }
    }

    /// Returns the reflected dunder method name for the operator.
    pub const fn reflected_dunder(self) -> &'static str {
        match self {
            Operator::Add => "__radd__",
            Operator::Sub => "__rsub__",
            Operator::Mult => "__rmul__",
            Operator::MatMult => "__rmatmul__",
            Operator::Div => "__rtruediv__",
            Operator::Mod => "__rmod__",
            Operator::Pow => "__rpow__",
            Operator::LShift => "__rlshift__",
            Operator::RShift => "__rrshift__",
            Operator::BitOr => "__ror__",
            Operator::BitXor => "__rxor__",
            Operator::BitAnd => "__rand__",
            Operator::FloorDiv => "__rfloordiv__",
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [unaryop](https://docs.python.org/3/library/ast.html#ast.unaryop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum UnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}

impl UnaryOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [cmpop](https://docs.python.org/3/library/ast.html#ast.cmpop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl CmpOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }

    #[must_use]
    pub const fn negate(&self) -> Self {
        match self {
            CmpOp::Eq => CmpOp::NotEq,
            CmpOp::NotEq => CmpOp::Eq,
            CmpOp::Lt => CmpOp::GtE,
            CmpOp::LtE => CmpOp::Gt,
            CmpOp::Gt => CmpOp::LtE,
            CmpOp::GtE => CmpOp::Lt,
            CmpOp::Is => CmpOp::IsNot,
            CmpOp::IsNot => CmpOp::Is,
            CmpOp::In => CmpOp::NotIn,
            CmpOp::NotIn => CmpOp::In,
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct IrrefutablePattern {
    pub kind: IrrefutablePatternKind,
    pub range: TextRange,
    pub node_index: NodeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum IrrefutablePatternKind {
    Name(Name),
    Wildcard,
}

/// The kind of escape command as defined in [IPython Syntax] in the IPython codebase.
///
/// [IPython Syntax]: https://github.com/ipython/ipython/blob/635815e8f1ded5b764d66cacc80bbe25e9e2587f/IPython/core/inputtransformer2.py#L335-L343
#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum IpyEscapeKind {
    /// Send line to underlying system shell (`!`).
    Shell,
    /// Send line to system shell and capture output (`!!`).
    ShCap,
    /// Show help on object (`?`).
    Help,
    /// Show help on object, with extra verbosity (`??`).
    Help2,
    /// Call magic function (`%`).
    Magic,
    /// Call cell magic function (`%%`).
    Magic2,
    /// Call first argument with rest of line as arguments after splitting on whitespace
    /// and quote each as string (`,`).
    Quote,
    /// Call first argument with rest of line as an argument quoted as a single string (`;`).
    Quote2,
    /// Call first argument with rest of line as arguments (`/`).
    Paren,
}

impl TryFrom<char> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: char) -> Result<Self, Self::Error> {
        match ch {
            '!' => Ok(IpyEscapeKind::Shell),
            '?' => Ok(IpyEscapeKind::Help),
            '%' => Ok(IpyEscapeKind::Magic),
            ',' => Ok(IpyEscapeKind::Quote),
            ';' => Ok(IpyEscapeKind::Quote2),
            '/' => Ok(IpyEscapeKind::Paren),
            _ => Err(format!("Unexpected magic escape: {ch}")),
        }
    }
}

impl TryFrom<[char; 2]> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: [char; 2]) -> Result<Self, Self::Error> {
        match ch {
            ['!', '!'] => Ok(IpyEscapeKind::ShCap),
            ['?', '?'] => Ok(IpyEscapeKind::Help2),
            ['%', '%'] => Ok(IpyEscapeKind::Magic2),
            [c1, c2] => Err(format!("Unexpected magic escape: {c1}{c2}")),
        }
    }
}

impl fmt::Display for IpyEscapeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl IpyEscapeKind {
    /// Returns `true` if the escape kind is help i.e., `?` or `??`.
    pub const fn is_help(self) -> bool {
        matches!(self, IpyEscapeKind::Help | IpyEscapeKind::Help2)
    }

    /// Returns `true` if the escape kind is magic i.e., `%` or `%%`.
    pub const fn is_magic(self) -> bool {
        matches!(self, IpyEscapeKind::Magic | IpyEscapeKind::Magic2)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IpyEscapeKind::Shell => "!",
            IpyEscapeKind::ShCap => "!!",
            IpyEscapeKind::Help => "?",
            IpyEscapeKind::Help2 => "??",
            IpyEscapeKind::Magic => "%",
            IpyEscapeKind::Magic2 => "%%",
            IpyEscapeKind::Quote => ",",
            IpyEscapeKind::Quote2 => ";",
            IpyEscapeKind::Paren => "/",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Singleton {
    None,
    True,
    False,
}

impl From<bool> for Singleton {
    fn from(value: bool) -> Self {
        if value {
            Singleton::True
        } else {
            Singleton::False
        }
    }
}
