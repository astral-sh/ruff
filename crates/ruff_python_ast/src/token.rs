//! Token kinds for Python source code created by the lexer and consumed by the `ruff_python_parser`.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Grammar/Tokens

use std::fmt;

use bitflags::bitflags;

use crate::str::{Quote, TripleQuotes};
use crate::str_prefix::{
    AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix, TStringPrefix,
};
use crate::{AnyStringFlags, BoolOp, Operator, StringFlags, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};

mod parentheses;
mod tokens;

pub use parentheses::{parentheses_iterator, parenthesized_range};
pub use tokens::{TokenAt, TokenIterWithContext, Tokens};

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct Token {
    /// The start and end offsets as native-endian bytes.
    ///
    /// Byte arrays keep the token byte-aligned instead of adding two bytes of trailing padding.
    start: [u8; 4],
    end: [u8; 4],
    /// The token kind and flags, encoded together because flags are only used by strings,
    /// non-ASCII names, and a few recovery tokens.
    kind_and_flags: u8,
}

impl Token {
    pub fn new(kind: TokenKind, range: TextRange, flags: TokenFlags) -> Token {
        Self {
            start: u32::from(range.start()).to_ne_bytes(),
            end: u32::from(range.end()).to_ne_bytes(),
            kind_and_flags: Self::encode_kind_and_flags(kind, flags),
        }
    }

    /// Returns the token kind.
    #[inline]
    pub const fn kind(&self) -> TokenKind {
        self.kind_and_flags().0
    }

    /// Returns the token as a tuple of (kind, range).
    #[inline]
    pub const fn as_tuple(&self) -> (TokenKind, TextRange) {
        (self.kind(), self.range())
    }

    /// Returns `true` if the current token is a triple-quoted string of any kind.
    ///
    /// # Panics
    ///
    /// If it isn't a string or any f/t-string tokens.
    pub fn is_triple_quoted_string(self) -> bool {
        self.unwrap_string_flags().is_triple_quoted()
    }

    /// Returns the [`Quote`] style for the current string token of any kind.
    ///
    /// # Panics
    ///
    /// If it isn't a string or any f/t-string tokens.
    pub fn string_quote_style(self) -> Quote {
        self.unwrap_string_flags().quote_style()
    }

    /// Returns the [`AnyStringFlags`] style for the current string token of any kind.
    ///
    /// # Panics
    ///
    /// If it isn't a string or any f/t-string tokens.
    pub fn unwrap_string_flags(self) -> AnyStringFlags {
        self.string_flags()
            .unwrap_or_else(|| panic!("token to be a string"))
    }

    /// Returns true if the current token is a string and it is raw.
    pub fn string_flags(self) -> Option<AnyStringFlags> {
        let (kind, flags) = self.kind_and_flags();
        if matches!(
            kind,
            TokenKind::String
                | TokenKind::FStringStart
                | TokenKind::FStringMiddle
                | TokenKind::FStringEnd
                | TokenKind::TStringStart
                | TokenKind::TStringMiddle
                | TokenKind::TStringEnd
        ) {
            Some(flags.as_any_string_flags())
        } else {
            None
        }
    }

    const fn range(&self) -> TextRange {
        TextRange::new(
            TextSize::new(u32::from_ne_bytes(self.start)),
            TextSize::new(u32::from_ne_bytes(self.end)),
        )
    }

    const NON_ASCII_NAME: u8 = TokenKind::Unknown as u8 + 1;
    const STRING_START: u8 = Self::NON_ASCII_NAME + 1;
    const INTERPOLATED_STRING_START: u8 = Self::STRING_START + 7 * 8;
    const UNCLOSED_NEWLINE: u8 = Self::INTERPOLATED_STRING_START + 6 * 12;
    const UNCLOSED_NON_LOGICAL_NEWLINE: u8 = Self::UNCLOSED_NEWLINE + 1;
    const UNCLOSED_UNKNOWN: u8 = Self::UNCLOSED_NON_LOGICAL_NEWLINE + 1;
    const UNCLOSED_INTERPOLATED_STRING_START: u8 = Self::UNCLOSED_UNKNOWN + 1;

    /// Encodes flag-free kinds directly, followed by compact ranges for each valid flagged kind.
    fn encode_kind_and_flags(kind: TokenKind, flags: TokenFlags) -> u8 {
        if flags.is_empty() {
            return kind as u8;
        }
        if kind == TokenKind::Name && flags == TokenFlags::NON_ASCII_NAME {
            return Self::NON_ASCII_NAME;
        }
        if kind == TokenKind::Newline && flags == TokenFlags::UNCLOSED_STRING {
            return Self::UNCLOSED_NEWLINE;
        }
        if kind == TokenKind::NonLogicalNewline && flags == TokenFlags::UNCLOSED_STRING {
            return Self::UNCLOSED_NON_LOGICAL_NEWLINE;
        }
        if kind == TokenKind::Unknown && flags == TokenFlags::UNCLOSED_STRING {
            return Self::UNCLOSED_UNKNOWN;
        }

        if kind == TokenKind::String {
            let prefix = match flags.prefix() {
                AnyStringPrefix::Regular(StringLiteralPrefix::Empty) => 0,
                AnyStringPrefix::Regular(StringLiteralPrefix::Unicode) => 1,
                AnyStringPrefix::Bytes(ByteStringPrefix::Regular) => 2,
                AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false }) => 3,
                AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true }) => 4,
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false }) => 5,
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true }) => 6,
                _ => unreachable!("non-interpolated string has an interpolated prefix"),
            };
            debug_assert!(!flags.is_non_ascii_name());
            return Self::STRING_START + prefix * 8 + flags.string_status();
        }

        let phase = match kind {
            TokenKind::FStringStart => Some(0),
            TokenKind::FStringMiddle => Some(1),
            TokenKind::FStringEnd => Some(2),
            TokenKind::TStringStart => Some(3),
            TokenKind::TStringMiddle => Some(4),
            TokenKind::TStringEnd => Some(5),
            _ => None,
        };
        if let Some(phase) = phase {
            if flags == TokenFlags::UNCLOSED_STRING {
                return Self::UNCLOSED_INTERPOLATED_STRING_START + phase;
            }
            let prefix = match flags.prefix() {
                AnyStringPrefix::Format(FStringPrefix::Regular)
                | AnyStringPrefix::Template(TStringPrefix::Regular) => 0,
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false })
                | AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: false }) => 1,
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true })
                | AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: true }) => 2,
                _ => unreachable!(
                    "interpolated string has a non-interpolated prefix: {kind:?} {flags:?}"
                ),
            };
            debug_assert!(!flags.is_unclosed());
            debug_assert!(!flags.is_non_ascii_name());
            return Self::INTERPOLATED_STRING_START
                + phase * 12
                + prefix * 4
                + flags.quote_status();
        }

        debug_assert!(flags.is_empty(), "{kind:?} {flags:?}");
        kind as u8
    }

    const fn interpolated_string_kind(phase: u8) -> TokenKind {
        match phase {
            0 => TokenKind::FStringStart,
            1 => TokenKind::FStringMiddle,
            2 => TokenKind::FStringEnd,
            3 => TokenKind::TStringStart,
            4 => TokenKind::TStringMiddle,
            5 => TokenKind::TStringEnd,
            _ => unreachable!(),
        }
    }

    const fn kind_and_flags(&self) -> (TokenKind, TokenFlags) {
        let encoded = self.kind_and_flags;
        if let Some(kind) = TokenKind::from_repr(encoded) {
            return (kind, TokenFlags::empty());
        }
        if encoded == Self::NON_ASCII_NAME {
            return (TokenKind::Name, TokenFlags::NON_ASCII_NAME);
        }
        if encoded == Self::UNCLOSED_NEWLINE {
            return (TokenKind::Newline, TokenFlags::UNCLOSED_STRING);
        }
        if encoded == Self::UNCLOSED_NON_LOGICAL_NEWLINE {
            return (TokenKind::NonLogicalNewline, TokenFlags::UNCLOSED_STRING);
        }
        if encoded == Self::UNCLOSED_UNKNOWN {
            return (TokenKind::Unknown, TokenFlags::UNCLOSED_STRING);
        }
        if encoded >= Self::UNCLOSED_INTERPOLATED_STRING_START {
            return (
                Self::interpolated_string_kind(encoded - Self::UNCLOSED_INTERPOLATED_STRING_START),
                TokenFlags::UNCLOSED_STRING,
            );
        }
        if encoded < Self::INTERPOLATED_STRING_START {
            let encoded = encoded - Self::STRING_START;
            let prefix = match encoded / 8 {
                0 => TokenFlags::empty(),
                1 => TokenFlags::UNICODE_STRING,
                2 => TokenFlags::BYTE_STRING,
                3 => TokenFlags::RAW_STRING_LOWERCASE,
                4 => TokenFlags::RAW_STRING_UPPERCASE,
                5 => TokenFlags::BYTE_STRING.union(TokenFlags::RAW_STRING_LOWERCASE),
                6 => TokenFlags::BYTE_STRING.union(TokenFlags::RAW_STRING_UPPERCASE),
                _ => unreachable!(),
            };
            return (
                TokenKind::String,
                TokenFlags::with_string_status(prefix, encoded % 8),
            );
        }

        let encoded = encoded - Self::INTERPOLATED_STRING_START;
        let phase = encoded / 12;
        let prefix = (encoded % 12) / 4;
        let status = encoded % 4;
        let kind = Self::interpolated_string_kind(phase);
        let prefix = match (phase < 3, prefix) {
            (true, 0) => TokenFlags::F_STRING,
            (true, 1) => TokenFlags::F_STRING.union(TokenFlags::RAW_STRING_LOWERCASE),
            (true, 2) => TokenFlags::F_STRING.union(TokenFlags::RAW_STRING_UPPERCASE),
            (false, 0) => TokenFlags::T_STRING,
            (false, 1) => TokenFlags::T_STRING.union(TokenFlags::RAW_STRING_LOWERCASE),
            (false, 2) => TokenFlags::T_STRING.union(TokenFlags::RAW_STRING_UPPERCASE),
            _ => unreachable!(),
        };
        (kind, TokenFlags::with_string_status(prefix, status))
    }
}

impl Ranged for Token {
    fn range(&self) -> TextRange {
        self.range()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, flags) = self.kind_and_flags();
        write!(f, "{kind:?} {:?}", self.range())?;
        if !flags.is_empty() {
            f.write_str(" (flags = ")?;
            let mut first = true;
            for (name, _) in flags.iter_names() {
                if first {
                    first = false;
                } else {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

/// A kind of a token.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, strum_macros::FromRepr)]
#[repr(u8)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum TokenKind {
    /// Token kind for a name, commonly known as an identifier.
    Name,
    /// Token kind for an integer.
    Int,
    /// Token kind for a floating point number.
    Float,
    /// Token kind for a complex number.
    Complex,
    /// Token kind for a string.
    String,
    /// Token kind for the start of an f-string. This includes the `f`/`F`/`fr` prefix
    /// and the opening quote(s).
    FStringStart,
    /// Token kind that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    FStringMiddle,
    /// Token kind for the end of an f-string. This includes the closing quote.
    FStringEnd,
    /// Token kind for the start of a t-string. This includes the `t`/`T`/`tr` prefix
    /// and the opening quote(s).
    TStringStart,
    /// Token kind that includes the portion of text inside the t-string that's not
    /// part of the interpolation part and isn't an opening or closing brace.
    TStringMiddle,
    /// Token kind for the end of a t-string. This includes the closing quote.
    TStringEnd,
    /// Token kind for a IPython escape command.
    IpyEscapeCommand,
    /// Token kind for a comment. These are filtered out of the token stream prior to parsing.
    Comment,
    /// Token kind for a newline.
    Newline,
    /// Token kind for a newline that is not a logical line break. These are filtered out of
    /// the token stream prior to parsing.
    NonLogicalNewline,
    /// Token kind for an indent.
    Indent,
    /// Token kind for a dedent.
    Dedent,
    EndOfFile,
    /// Token kind for a question mark `?`.
    Question,
    /// Token kind for an exclamation mark `!`.
    Exclamation,
    /// Token kind for a left parenthesis `(`.
    Lpar,
    /// Token kind for a right parenthesis `)`.
    Rpar,
    /// Token kind for a left square bracket `[`.
    Lsqb,
    /// Token kind for a right square bracket `]`.
    Rsqb,
    /// Token kind for a colon `:`.
    Colon,
    /// Token kind for a comma `,`.
    Comma,
    /// Token kind for a semicolon `;`.
    Semi,
    /// Token kind for plus `+`.
    Plus,
    /// Token kind for minus `-`.
    Minus,
    /// Token kind for star `*`.
    Star,
    /// Token kind for slash `/`.
    Slash,
    /// Token kind for vertical bar `|`.
    Vbar,
    /// Token kind for ampersand `&`.
    Amper,
    /// Token kind for less than `<`.
    Less,
    /// Token kind for greater than `>`.
    Greater,
    /// Token kind for equal `=`.
    Equal,
    /// Token kind for dot `.`.
    Dot,
    /// Token kind for percent `%`.
    Percent,
    /// Token kind for left bracket `{`.
    Lbrace,
    /// Token kind for right bracket `}`.
    Rbrace,
    /// Token kind for double equal `==`.
    EqEqual,
    /// Token kind for not equal `!=`.
    NotEqual,
    /// Token kind for less than or equal `<=`.
    LessEqual,
    /// Token kind for greater than or equal `>=`.
    GreaterEqual,
    /// Token kind for tilde `~`.
    Tilde,
    /// Token kind for caret `^`.
    CircumFlex,
    /// Token kind for left shift `<<`.
    LeftShift,
    /// Token kind for right shift `>>`.
    RightShift,
    /// Token kind for double star `**`.
    DoubleStar,
    /// Token kind for double star equal `**=`.
    DoubleStarEqual,
    /// Token kind for plus equal `+=`.
    PlusEqual,
    /// Token kind for minus equal `-=`.
    MinusEqual,
    /// Token kind for star equal `*=`.
    StarEqual,
    /// Token kind for slash equal `/=`.
    SlashEqual,
    /// Token kind for percent equal `%=`.
    PercentEqual,
    /// Token kind for ampersand equal `&=`.
    AmperEqual,
    /// Token kind for vertical bar equal `|=`.
    VbarEqual,
    /// Token kind for caret equal `^=`.
    CircumflexEqual,
    /// Token kind for left shift equal `<<=`.
    LeftShiftEqual,
    /// Token kind for right shift equal `>>=`.
    RightShiftEqual,
    /// Token kind for double slash `//`.
    DoubleSlash,
    /// Token kind for double slash equal `//=`.
    DoubleSlashEqual,
    /// Token kind for colon equal `:=`.
    ColonEqual,
    /// Token kind for at `@`.
    At,
    /// Token kind for at equal `@=`.
    AtEqual,
    /// Token kind for arrow `->`.
    Rarrow,
    /// Token kind for ellipsis `...`.
    Ellipsis,

    // The keywords should be sorted in alphabetical order. If the boundary tokens for the
    // "Keywords" and "Soft keywords" group change, update the related methods on `TokenKind`.

    // Keywords
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Class,
    Continue,
    Def,
    Del,
    Elif,
    Else,
    Except,
    False,
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    None,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    True,
    Try,
    While,
    With,
    Yield,

    // Soft keywords
    Case,
    Lazy,
    Match,
    Type,

    Unknown,
}

impl TokenKind {
    /// Returns `true` if this is an end of file token.
    #[inline]
    pub const fn is_eof(self) -> bool {
        matches!(self, TokenKind::EndOfFile)
    }

    /// Returns `true` if this is either a newline or non-logical newline token.
    #[inline]
    pub const fn is_any_newline(self) -> bool {
        matches!(self, TokenKind::Newline | TokenKind::NonLogicalNewline)
    }

    /// Returns `true` if the token is a keyword (including soft keywords).
    ///
    /// See also [`is_soft_keyword`], [`is_non_soft_keyword`].
    ///
    /// [`is_soft_keyword`]: TokenKind::is_soft_keyword
    /// [`is_non_soft_keyword`]: TokenKind::is_non_soft_keyword
    #[inline]
    pub fn is_keyword(self) -> bool {
        TokenKind::And <= self && self <= TokenKind::Type
    }

    /// Returns `true` if the token is strictly a soft keyword.
    ///
    /// See also [`is_keyword`], [`is_non_soft_keyword`].
    ///
    /// [`is_keyword`]: TokenKind::is_keyword
    /// [`is_non_soft_keyword`]: TokenKind::is_non_soft_keyword
    #[inline]
    pub fn is_soft_keyword(self) -> bool {
        TokenKind::Case <= self && self <= TokenKind::Type
    }

    /// Returns `true` if the token is strictly a non-soft keyword.
    ///
    /// See also [`is_keyword`], [`is_soft_keyword`].
    ///
    /// [`is_keyword`]: TokenKind::is_keyword
    /// [`is_soft_keyword`]: TokenKind::is_soft_keyword
    #[inline]
    pub fn is_non_soft_keyword(self) -> bool {
        TokenKind::And <= self && self <= TokenKind::Yield
    }

    #[inline]
    pub const fn is_operator(self) -> bool {
        matches!(
            self,
            TokenKind::Lpar
                | TokenKind::Rpar
                | TokenKind::Lsqb
                | TokenKind::Rsqb
                | TokenKind::Comma
                | TokenKind::Semi
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Vbar
                | TokenKind::Amper
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::Equal
                | TokenKind::Dot
                | TokenKind::Percent
                | TokenKind::Lbrace
                | TokenKind::Rbrace
                | TokenKind::EqEqual
                | TokenKind::NotEqual
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::Tilde
                | TokenKind::CircumFlex
                | TokenKind::LeftShift
                | TokenKind::RightShift
                | TokenKind::DoubleStar
                | TokenKind::PlusEqual
                | TokenKind::MinusEqual
                | TokenKind::StarEqual
                | TokenKind::SlashEqual
                | TokenKind::PercentEqual
                | TokenKind::AmperEqual
                | TokenKind::VbarEqual
                | TokenKind::CircumflexEqual
                | TokenKind::LeftShiftEqual
                | TokenKind::RightShiftEqual
                | TokenKind::DoubleStarEqual
                | TokenKind::DoubleSlash
                | TokenKind::DoubleSlashEqual
                | TokenKind::At
                | TokenKind::AtEqual
                | TokenKind::Rarrow
                | TokenKind::Ellipsis
                | TokenKind::ColonEqual
                | TokenKind::Colon
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Not
                | TokenKind::In
                | TokenKind::Is
        )
    }

    /// Returns `true` if this is a singleton token i.e., `True`, `False`, or `None`.
    #[inline]
    pub const fn is_singleton(self) -> bool {
        matches!(self, TokenKind::False | TokenKind::True | TokenKind::None)
    }

    /// Returns `true` if this is a trivia token i.e., a comment or a non-logical newline.
    #[inline]
    pub const fn is_trivia(&self) -> bool {
        matches!(self, TokenKind::Comment | TokenKind::NonLogicalNewline)
    }

    /// Returns `true` if this is a comment token.
    #[inline]
    pub const fn is_comment(&self) -> bool {
        matches!(self, TokenKind::Comment)
    }

    #[inline]
    pub const fn is_arithmetic(self) -> bool {
        matches!(
            self,
            TokenKind::DoubleStar
                | TokenKind::Star
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Slash
                | TokenKind::DoubleSlash
                | TokenKind::At
        )
    }

    #[inline]
    pub const fn is_bitwise_or_shift(self) -> bool {
        matches!(
            self,
            TokenKind::LeftShift
                | TokenKind::LeftShiftEqual
                | TokenKind::RightShift
                | TokenKind::RightShiftEqual
                | TokenKind::Amper
                | TokenKind::AmperEqual
                | TokenKind::Vbar
                | TokenKind::VbarEqual
                | TokenKind::CircumFlex
                | TokenKind::CircumflexEqual
                | TokenKind::Tilde
        )
    }

    /// Returns `true` if the current token is a unary arithmetic operator.
    #[inline]
    pub const fn is_unary_arithmetic_operator(self) -> bool {
        matches!(self, TokenKind::Plus | TokenKind::Minus)
    }

    #[inline]
    pub const fn is_interpolated_string_end(self) -> bool {
        matches!(self, TokenKind::FStringEnd | TokenKind::TStringEnd)
    }

    /// Returns the [`UnaryOp`] that corresponds to this token kind, if it is a unary arithmetic
    /// operator, otherwise return [None].
    ///
    /// Use [`as_unary_operator`] to match against any unary operator.
    ///
    /// [`as_unary_operator`]: TokenKind::as_unary_operator
    #[inline]
    pub const fn as_unary_arithmetic_operator(self) -> Option<UnaryOp> {
        Some(match self {
            TokenKind::Plus => UnaryOp::UAdd,
            TokenKind::Minus => UnaryOp::USub,
            _ => return None,
        })
    }

    /// Returns the [`UnaryOp`] that corresponds to this token kind, if it is a unary operator,
    /// otherwise return [None].
    ///
    /// Use [`as_unary_arithmetic_operator`] to match against only an arithmetic unary operator.
    ///
    /// [`as_unary_arithmetic_operator`]: TokenKind::as_unary_arithmetic_operator
    #[inline]
    pub const fn as_unary_operator(self) -> Option<UnaryOp> {
        Some(match self {
            TokenKind::Plus => UnaryOp::UAdd,
            TokenKind::Minus => UnaryOp::USub,
            TokenKind::Tilde => UnaryOp::Invert,
            TokenKind::Not => UnaryOp::Not,
            _ => return None,
        })
    }

    /// Returns the [`BoolOp`] that corresponds to this token kind, if it is a boolean operator,
    /// otherwise return [None].
    #[inline]
    pub const fn as_bool_operator(self) -> Option<BoolOp> {
        Some(match self {
            TokenKind::And => BoolOp::And,
            TokenKind::Or => BoolOp::Or,
            _ => return None,
        })
    }

    /// Returns the binary [`Operator`] that corresponds to the current token, if it's a binary
    /// operator, otherwise return [None].
    ///
    /// Use [`as_augmented_assign_operator`] to match against an augmented assignment token.
    ///
    /// [`as_augmented_assign_operator`]: TokenKind::as_augmented_assign_operator
    pub const fn as_binary_operator(self) -> Option<Operator> {
        Some(match self {
            TokenKind::Plus => Operator::Add,
            TokenKind::Minus => Operator::Sub,
            TokenKind::Star => Operator::Mult,
            TokenKind::At => Operator::MatMult,
            TokenKind::DoubleStar => Operator::Pow,
            TokenKind::Slash => Operator::Div,
            TokenKind::DoubleSlash => Operator::FloorDiv,
            TokenKind::Percent => Operator::Mod,
            TokenKind::Amper => Operator::BitAnd,
            TokenKind::Vbar => Operator::BitOr,
            TokenKind::CircumFlex => Operator::BitXor,
            TokenKind::LeftShift => Operator::LShift,
            TokenKind::RightShift => Operator::RShift,
            _ => return None,
        })
    }

    /// Returns the [`Operator`] that corresponds to this token kind, if it is
    /// an augmented assignment operator, or [`None`] otherwise.
    #[inline]
    pub const fn as_augmented_assign_operator(self) -> Option<Operator> {
        Some(match self {
            TokenKind::PlusEqual => Operator::Add,
            TokenKind::MinusEqual => Operator::Sub,
            TokenKind::StarEqual => Operator::Mult,
            TokenKind::AtEqual => Operator::MatMult,
            TokenKind::DoubleStarEqual => Operator::Pow,
            TokenKind::SlashEqual => Operator::Div,
            TokenKind::DoubleSlashEqual => Operator::FloorDiv,
            TokenKind::PercentEqual => Operator::Mod,
            TokenKind::AmperEqual => Operator::BitAnd,
            TokenKind::VbarEqual => Operator::BitOr,
            TokenKind::CircumflexEqual => Operator::BitXor,
            TokenKind::LeftShiftEqual => Operator::LShift,
            TokenKind::RightShiftEqual => Operator::RShift,
            _ => return None,
        })
    }
}

impl From<BoolOp> for TokenKind {
    #[inline]
    fn from(op: BoolOp) -> Self {
        match op {
            BoolOp::And => TokenKind::And,
            BoolOp::Or => TokenKind::Or,
        }
    }
}

impl From<UnaryOp> for TokenKind {
    #[inline]
    fn from(op: UnaryOp) -> Self {
        match op {
            UnaryOp::Invert => TokenKind::Tilde,
            UnaryOp::Not => TokenKind::Not,
            UnaryOp::UAdd => TokenKind::Plus,
            UnaryOp::USub => TokenKind::Minus,
        }
    }
}

impl From<Operator> for TokenKind {
    #[inline]
    fn from(op: Operator) -> Self {
        match op {
            Operator::Add => TokenKind::Plus,
            Operator::Sub => TokenKind::Minus,
            Operator::Mult => TokenKind::Star,
            Operator::MatMult => TokenKind::At,
            Operator::Div => TokenKind::Slash,
            Operator::Mod => TokenKind::Percent,
            Operator::Pow => TokenKind::DoubleStar,
            Operator::LShift => TokenKind::LeftShift,
            Operator::RShift => TokenKind::RightShift,
            Operator::BitOr => TokenKind::Vbar,
            Operator::BitXor => TokenKind::CircumFlex,
            Operator::BitAnd => TokenKind::Amper,
            Operator::FloorDiv => TokenKind::DoubleSlash,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            TokenKind::Unknown => "Unknown",
            TokenKind::Newline => "newline",
            TokenKind::NonLogicalNewline => "NonLogicalNewline",
            TokenKind::Indent => "indent",
            TokenKind::Dedent => "dedent",
            TokenKind::EndOfFile => "end of file",
            TokenKind::Name => "name",
            TokenKind::Int => "int",
            TokenKind::Float => "float",
            TokenKind::Complex => "complex",
            TokenKind::String => "string",
            TokenKind::FStringStart => "FStringStart",
            TokenKind::FStringMiddle => "FStringMiddle",
            TokenKind::FStringEnd => "FStringEnd",
            TokenKind::TStringStart => "TStringStart",
            TokenKind::TStringMiddle => "TStringMiddle",
            TokenKind::TStringEnd => "TStringEnd",
            TokenKind::IpyEscapeCommand => "IPython escape command",
            TokenKind::Comment => "comment",
            TokenKind::Question => "`?`",
            TokenKind::Exclamation => "`!`",
            TokenKind::Lpar => "`(`",
            TokenKind::Rpar => "`)`",
            TokenKind::Lsqb => "`[`",
            TokenKind::Rsqb => "`]`",
            TokenKind::Lbrace => "`{`",
            TokenKind::Rbrace => "`}`",
            TokenKind::Equal => "`=`",
            TokenKind::ColonEqual => "`:=`",
            TokenKind::Dot => "`.`",
            TokenKind::Colon => "`:`",
            TokenKind::Semi => "`;`",
            TokenKind::Comma => "`,`",
            TokenKind::Rarrow => "`->`",
            TokenKind::Plus => "`+`",
            TokenKind::Minus => "`-`",
            TokenKind::Star => "`*`",
            TokenKind::DoubleStar => "`**`",
            TokenKind::Slash => "`/`",
            TokenKind::DoubleSlash => "`//`",
            TokenKind::Percent => "`%`",
            TokenKind::Vbar => "`|`",
            TokenKind::Amper => "`&`",
            TokenKind::CircumFlex => "`^`",
            TokenKind::LeftShift => "`<<`",
            TokenKind::RightShift => "`>>`",
            TokenKind::Tilde => "`~`",
            TokenKind::At => "`@`",
            TokenKind::Less => "`<`",
            TokenKind::Greater => "`>`",
            TokenKind::EqEqual => "`==`",
            TokenKind::NotEqual => "`!=`",
            TokenKind::LessEqual => "`<=`",
            TokenKind::GreaterEqual => "`>=`",
            TokenKind::PlusEqual => "`+=`",
            TokenKind::MinusEqual => "`-=`",
            TokenKind::StarEqual => "`*=`",
            TokenKind::DoubleStarEqual => "`**=`",
            TokenKind::SlashEqual => "`/=`",
            TokenKind::DoubleSlashEqual => "`//=`",
            TokenKind::PercentEqual => "`%=`",
            TokenKind::VbarEqual => "`|=`",
            TokenKind::AmperEqual => "`&=`",
            TokenKind::CircumflexEqual => "`^=`",
            TokenKind::LeftShiftEqual => "`<<=`",
            TokenKind::RightShiftEqual => "`>>=`",
            TokenKind::AtEqual => "`@=`",
            TokenKind::Ellipsis => "`...`",
            TokenKind::False => "`False`",
            TokenKind::None => "`None`",
            TokenKind::True => "`True`",
            TokenKind::And => "`and`",
            TokenKind::As => "`as`",
            TokenKind::Assert => "`assert`",
            TokenKind::Async => "`async`",
            TokenKind::Await => "`await`",
            TokenKind::Break => "`break`",
            TokenKind::Class => "`class`",
            TokenKind::Continue => "`continue`",
            TokenKind::Def => "`def`",
            TokenKind::Del => "`del`",
            TokenKind::Elif => "`elif`",
            TokenKind::Else => "`else`",
            TokenKind::Except => "`except`",
            TokenKind::Finally => "`finally`",
            TokenKind::For => "`for`",
            TokenKind::From => "`from`",
            TokenKind::Global => "`global`",
            TokenKind::If => "`if`",
            TokenKind::Import => "`import`",
            TokenKind::In => "`in`",
            TokenKind::Is => "`is`",
            TokenKind::Lambda => "`lambda`",
            TokenKind::Nonlocal => "`nonlocal`",
            TokenKind::Not => "`not`",
            TokenKind::Or => "`or`",
            TokenKind::Pass => "`pass`",
            TokenKind::Raise => "`raise`",
            TokenKind::Return => "`return`",
            TokenKind::Try => "`try`",
            TokenKind::While => "`while`",
            TokenKind::Lazy => "`lazy`",
            TokenKind::Match => "`match`",
            TokenKind::Type => "`type`",
            TokenKind::Case => "`case`",
            TokenKind::With => "`with`",
            TokenKind::Yield => "`yield`",
        };
        f.write_str(value)
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TokenFlags: u16 {
        /// The token is a string with double quotes (`"`).
        const DOUBLE_QUOTES = 1 << 0;
        /// The token is a triple-quoted string i.e., it starts and ends with three consecutive
        /// quote characters (`"""` or `'''`).
        const TRIPLE_QUOTED_STRING = 1 << 1;

        /// The token is a unicode string i.e., prefixed with `u` or `U`
        const UNICODE_STRING = 1 << 2;
        /// The token is a byte string i.e., prefixed with `b` or `B`
        const BYTE_STRING = 1 << 3;
        /// The token is an f-string i.e., prefixed with `f` or `F`
        const F_STRING = 1 << 4;
        /// The token is a t-string i.e., prefixed with `t` or `T`
        const T_STRING = 1 << 5;
        /// The token is a raw string and the prefix character is in lowercase.
        const RAW_STRING_LOWERCASE = 1 << 6;
        /// The token is a raw string and the prefix character is in uppercase.
        const RAW_STRING_UPPERCASE = 1 << 7;
        /// String without matching closing quote(s)
        const UNCLOSED_STRING = 1 << 8;
        /// The token is a name containing at least one non-ASCII codepoint.
        const NON_ASCII_NAME = 1 << 9;

        /// The token is a raw string i.e., prefixed with `r` or `R`
        const RAW_STRING = Self::RAW_STRING_LOWERCASE.bits() | Self::RAW_STRING_UPPERCASE.bits();
    }
}

impl TokenFlags {
    const fn string_status(self) -> u8 {
        self.contains(Self::DOUBLE_QUOTES) as u8
            | ((self.contains(Self::TRIPLE_QUOTED_STRING) as u8) << 1)
            | ((self.contains(Self::UNCLOSED_STRING) as u8) << 2)
    }

    const fn quote_status(self) -> u8 {
        self.string_status() & 0b11
    }

    const fn with_string_status(mut self, status: u8) -> Self {
        if status & 0b001 != 0 {
            self = self.union(Self::DOUBLE_QUOTES);
        }
        if status & 0b010 != 0 {
            self = self.union(Self::TRIPLE_QUOTED_STRING);
        }
        if status & 0b100 != 0 {
            self = self.union(Self::UNCLOSED_STRING);
        }
        self
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for TokenFlags {}

impl StringFlags for TokenFlags {
    fn quote_style(self) -> Quote {
        if self.intersects(TokenFlags::DOUBLE_QUOTES) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn triple_quotes(self) -> TripleQuotes {
        if self.intersects(TokenFlags::TRIPLE_QUOTED_STRING) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        if self.intersects(TokenFlags::F_STRING) {
            if self.intersects(TokenFlags::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(TokenFlags::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Format(FStringPrefix::Regular)
            }
        } else if self.intersects(TokenFlags::T_STRING) {
            if self.intersects(TokenFlags::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(TokenFlags::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Template(TStringPrefix::Regular)
            }
        } else if self.intersects(TokenFlags::BYTE_STRING) {
            if self.intersects(TokenFlags::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(TokenFlags::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Bytes(ByteStringPrefix::Regular)
            }
        } else if self.intersects(TokenFlags::RAW_STRING_LOWERCASE) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false })
        } else if self.intersects(TokenFlags::RAW_STRING_UPPERCASE) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true })
        } else if self.intersects(TokenFlags::UNICODE_STRING) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Unicode)
        } else {
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
        }
    }

    fn is_unclosed(self) -> bool {
        self.intersects(TokenFlags::UNCLOSED_STRING)
    }
}

impl TokenFlags {
    /// Returns `true` if the token is an f-string.
    pub const fn is_f_string(self) -> bool {
        self.intersects(TokenFlags::F_STRING)
    }

    /// Returns `true` if the token is a t-string.
    pub const fn is_t_string(self) -> bool {
        self.intersects(TokenFlags::T_STRING)
    }

    /// Returns `true` if the token is a t-string.
    pub const fn is_interpolated_string(self) -> bool {
        self.intersects(TokenFlags::T_STRING.union(TokenFlags::F_STRING))
    }

    /// Returns `true` if the token is a triple-quoted t-string.
    pub fn is_triple_quoted_interpolated_string(self) -> bool {
        self.intersects(TokenFlags::TRIPLE_QUOTED_STRING) && self.is_interpolated_string()
    }

    /// Returns `true` if the token is a raw string.
    pub const fn is_raw_string(self) -> bool {
        self.intersects(TokenFlags::RAW_STRING)
    }

    /// Returns `true` if the token is a name containing at least one non-ASCII codepoint.
    #[inline]
    pub const fn is_non_ascii_name(self) -> bool {
        self.intersects(TokenFlags::NON_ASCII_NAME)
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use ruff_text_size::{TextRange, TextSize};

    use super::{Token, TokenFlags, TokenKind};

    #[test]
    fn token_is_nine_bytes() {
        assert_eq!(size_of::<Token>(), 9);
    }

    #[test]
    fn kind_and_flags_round_trip() {
        fn assert_round_trip(kind: TokenKind, flags: TokenFlags) {
            let range = TextRange::new(TextSize::new(1), TextSize::new(u32::MAX));
            let token = Token::new(kind, range, flags);
            assert_eq!(token.kind_and_flags(), (kind, flags));
            assert_eq!(token.as_tuple(), (kind, range));
        }

        for encoded in u8::MIN..=u8::MAX {
            if let Some(kind) = TokenKind::from_repr(encoded) {
                assert_round_trip(kind, TokenFlags::empty());
            }
        }
        assert_round_trip(TokenKind::Name, TokenFlags::NON_ASCII_NAME);
        assert_round_trip(TokenKind::Newline, TokenFlags::UNCLOSED_STRING);
        assert_round_trip(TokenKind::NonLogicalNewline, TokenFlags::UNCLOSED_STRING);
        assert_round_trip(TokenKind::Unknown, TokenFlags::UNCLOSED_STRING);

        let string_prefixes = [
            TokenFlags::empty(),
            TokenFlags::UNICODE_STRING,
            TokenFlags::BYTE_STRING,
            TokenFlags::RAW_STRING_LOWERCASE,
            TokenFlags::RAW_STRING_UPPERCASE,
            TokenFlags::BYTE_STRING.union(TokenFlags::RAW_STRING_LOWERCASE),
            TokenFlags::BYTE_STRING.union(TokenFlags::RAW_STRING_UPPERCASE),
        ];
        for prefix in string_prefixes {
            for status in 0..8 {
                assert_round_trip(
                    TokenKind::String,
                    TokenFlags::with_string_status(prefix, status),
                );
            }
        }

        for (kind, prefix) in [
            (TokenKind::FStringStart, TokenFlags::F_STRING),
            (TokenKind::FStringMiddle, TokenFlags::F_STRING),
            (TokenKind::FStringEnd, TokenFlags::F_STRING),
            (TokenKind::TStringStart, TokenFlags::T_STRING),
            (TokenKind::TStringMiddle, TokenFlags::T_STRING),
            (TokenKind::TStringEnd, TokenFlags::T_STRING),
        ] {
            for raw in [
                TokenFlags::empty(),
                TokenFlags::RAW_STRING_LOWERCASE,
                TokenFlags::RAW_STRING_UPPERCASE,
            ] {
                for quote_status in 0..4 {
                    assert_round_trip(
                        kind,
                        TokenFlags::with_string_status(prefix.union(raw), quote_status),
                    );
                }
            }
            assert_round_trip(kind, TokenFlags::UNCLOSED_STRING);
        }
    }
}
