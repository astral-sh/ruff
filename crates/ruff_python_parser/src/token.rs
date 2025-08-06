//! Token kinds for Python source code created by the lexer and consumed by the `ruff_python_parser`.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Grammar/Tokens

use std::fmt;

use bitflags::bitflags;

use ruff_python_ast::name::Name;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_ast::str_prefix::{
    AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix, TStringPrefix,
};
use ruff_python_ast::{AnyStringFlags, BoolOp, Int, IpyEscapeKind, Operator, StringFlags, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

#[derive(Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct Token {
    /// The kind of the token.
    kind: TokenKind,
    /// The range of the token.
    range: TextRange,
    /// The set of flags describing this token.
    flags: TokenFlags,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, range: TextRange, flags: TokenFlags) -> Self {
        Self { kind, range, flags }
    }

    /// Returns the token kind.
    #[inline]
    pub const fn kind(&self) -> TokenKind {
        self.kind
    }

    /// Returns the token as a tuple of (kind, range).
    #[inline]
    pub const fn as_tuple(&self) -> (TokenKind, TextRange) {
        (self.kind, self.range)
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
        if self.is_any_string() {
            Some(self.flags.as_any_string_flags())
        } else {
            None
        }
    }

    /// Returns `true` if this is any kind of string token - including
    /// tokens in t-strings (which do not have type `str`).
    const fn is_any_string(self) -> bool {
        matches!(
            self.kind,
            TokenKind::String
                | TokenKind::FStringStart
                | TokenKind::FStringMiddle
                | TokenKind::FStringEnd
                | TokenKind::TStringStart
                | TokenKind::TStringMiddle
                | TokenKind::TStringEnd
        )
    }
}

impl Ranged for Token {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:?}", self.kind, self.range)?;
        if !self.flags.is_empty() {
            f.write_str(" (flags = ")?;
            let mut first = true;
            for (name, _) in self.flags.iter_names() {
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
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, get_size2::GetSize)]
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
    Match,
    Type,

    Unknown,
}

impl TokenKind {
    /// Returns `true` if this is an end of file token.
    #[inline]
    pub const fn is_eof(self) -> bool {
        matches!(self, Self::EndOfFile)
    }

    /// Returns `true` if this is either a newline or non-logical newline token.
    #[inline]
    pub const fn is_any_newline(self) -> bool {
        matches!(self, Self::Newline | Self::NonLogicalNewline)
    }

    /// Returns `true` if the token is a keyword (including soft keywords).
    ///
    /// See also [`is_soft_keyword`], [`is_non_soft_keyword`].
    ///
    /// [`is_soft_keyword`]: TokenKind::is_soft_keyword
    /// [`is_non_soft_keyword`]: TokenKind::is_non_soft_keyword
    #[inline]
    pub fn is_keyword(self) -> bool {
        Self::And <= self && self <= Self::Type
    }

    /// Returns `true` if the token is strictly a soft keyword.
    ///
    /// See also [`is_keyword`], [`is_non_soft_keyword`].
    ///
    /// [`is_keyword`]: TokenKind::is_keyword
    /// [`is_non_soft_keyword`]: TokenKind::is_non_soft_keyword
    #[inline]
    pub fn is_soft_keyword(self) -> bool {
        Self::Case <= self && self <= Self::Type
    }

    /// Returns `true` if the token is strictly a non-soft keyword.
    ///
    /// See also [`is_keyword`], [`is_soft_keyword`].
    ///
    /// [`is_keyword`]: TokenKind::is_keyword
    /// [`is_soft_keyword`]: TokenKind::is_soft_keyword
    #[inline]
    pub fn is_non_soft_keyword(self) -> bool {
        Self::And <= self && self <= Self::Yield
    }

    #[inline]
    pub const fn is_operator(self) -> bool {
        matches!(
            self,
            Self::Lpar
                | Self::Rpar
                | Self::Lsqb
                | Self::Rsqb
                | Self::Comma
                | Self::Semi
                | Self::Plus
                | Self::Minus
                | Self::Star
                | Self::Slash
                | Self::Vbar
                | Self::Amper
                | Self::Less
                | Self::Greater
                | Self::Equal
                | Self::Dot
                | Self::Percent
                | Self::Lbrace
                | Self::Rbrace
                | Self::EqEqual
                | Self::NotEqual
                | Self::LessEqual
                | Self::GreaterEqual
                | Self::Tilde
                | Self::CircumFlex
                | Self::LeftShift
                | Self::RightShift
                | Self::DoubleStar
                | Self::PlusEqual
                | Self::MinusEqual
                | Self::StarEqual
                | Self::SlashEqual
                | Self::PercentEqual
                | Self::AmperEqual
                | Self::VbarEqual
                | Self::CircumflexEqual
                | Self::LeftShiftEqual
                | Self::RightShiftEqual
                | Self::DoubleStarEqual
                | Self::DoubleSlash
                | Self::DoubleSlashEqual
                | Self::At
                | Self::AtEqual
                | Self::Rarrow
                | Self::Ellipsis
                | Self::ColonEqual
                | Self::Colon
                | Self::And
                | Self::Or
                | Self::Not
                | Self::In
                | Self::Is
        )
    }

    /// Returns `true` if this is a singleton token i.e., `True`, `False`, or `None`.
    #[inline]
    pub const fn is_singleton(self) -> bool {
        matches!(self, Self::False | Self::True | Self::None)
    }

    /// Returns `true` if this is a trivia token i.e., a comment or a non-logical newline.
    #[inline]
    pub const fn is_trivia(&self) -> bool {
        matches!(self, Self::Comment | Self::NonLogicalNewline)
    }

    /// Returns `true` if this is a comment token.
    #[inline]
    pub const fn is_comment(&self) -> bool {
        matches!(self, Self::Comment)
    }

    #[inline]
    pub const fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::DoubleStar
                | Self::Star
                | Self::Plus
                | Self::Minus
                | Self::Slash
                | Self::DoubleSlash
                | Self::At
        )
    }

    #[inline]
    pub const fn is_bitwise_or_shift(self) -> bool {
        matches!(
            self,
            Self::LeftShift
                | Self::LeftShiftEqual
                | Self::RightShift
                | Self::RightShiftEqual
                | Self::Amper
                | Self::AmperEqual
                | Self::Vbar
                | Self::VbarEqual
                | Self::CircumFlex
                | Self::CircumflexEqual
                | Self::Tilde
        )
    }

    /// Returns `true` if the current token is a unary arithmetic operator.
    #[inline]
    pub const fn is_unary_arithmetic_operator(self) -> bool {
        matches!(self, Self::Plus | Self::Minus)
    }

    #[inline]
    pub const fn is_interpolated_string_end(self) -> bool {
        matches!(self, Self::FStringEnd | Self::TStringEnd)
    }

    /// Returns the [`UnaryOp`] that corresponds to this token kind, if it is a unary arithmetic
    /// operator, otherwise return [None].
    ///
    /// Use [`as_unary_operator`] to match against any unary operator.
    ///
    /// [`as_unary_operator`]: TokenKind::as_unary_operator
    #[inline]
    pub(crate) const fn as_unary_arithmetic_operator(self) -> Option<UnaryOp> {
        Some(match self {
            Self::Plus => UnaryOp::UAdd,
            Self::Minus => UnaryOp::USub,
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
    pub(crate) const fn as_unary_operator(self) -> Option<UnaryOp> {
        Some(match self {
            Self::Plus => UnaryOp::UAdd,
            Self::Minus => UnaryOp::USub,
            Self::Tilde => UnaryOp::Invert,
            Self::Not => UnaryOp::Not,
            _ => return None,
        })
    }

    /// Returns the [`BoolOp`] that corresponds to this token kind, if it is a boolean operator,
    /// otherwise return [None].
    #[inline]
    pub(crate) const fn as_bool_operator(self) -> Option<BoolOp> {
        Some(match self {
            Self::And => BoolOp::And,
            Self::Or => BoolOp::Or,
            _ => return None,
        })
    }

    /// Returns the binary [`Operator`] that corresponds to the current token, if it's a binary
    /// operator, otherwise return [None].
    ///
    /// Use [`as_augmented_assign_operator`] to match against an augmented assignment token.
    ///
    /// [`as_augmented_assign_operator`]: TokenKind::as_augmented_assign_operator
    pub(crate) const fn as_binary_operator(self) -> Option<Operator> {
        Some(match self {
            Self::Plus => Operator::Add,
            Self::Minus => Operator::Sub,
            Self::Star => Operator::Mult,
            Self::At => Operator::MatMult,
            Self::DoubleStar => Operator::Pow,
            Self::Slash => Operator::Div,
            Self::DoubleSlash => Operator::FloorDiv,
            Self::Percent => Operator::Mod,
            Self::Amper => Operator::BitAnd,
            Self::Vbar => Operator::BitOr,
            Self::CircumFlex => Operator::BitXor,
            Self::LeftShift => Operator::LShift,
            Self::RightShift => Operator::RShift,
            _ => return None,
        })
    }

    /// Returns the [`Operator`] that corresponds to this token kind, if it is
    /// an augmented assignment operator, or [`None`] otherwise.
    #[inline]
    pub(crate) const fn as_augmented_assign_operator(self) -> Option<Operator> {
        Some(match self {
            Self::PlusEqual => Operator::Add,
            Self::MinusEqual => Operator::Sub,
            Self::StarEqual => Operator::Mult,
            Self::AtEqual => Operator::MatMult,
            Self::DoubleStarEqual => Operator::Pow,
            Self::SlashEqual => Operator::Div,
            Self::DoubleSlashEqual => Operator::FloorDiv,
            Self::PercentEqual => Operator::Mod,
            Self::AmperEqual => Operator::BitAnd,
            Self::VbarEqual => Operator::BitOr,
            Self::CircumflexEqual => Operator::BitXor,
            Self::LeftShiftEqual => Operator::LShift,
            Self::RightShiftEqual => Operator::RShift,
            _ => return None,
        })
    }
}

impl From<BoolOp> for TokenKind {
    #[inline]
    fn from(op: BoolOp) -> Self {
        match op {
            BoolOp::And => Self::And,
            BoolOp::Or => Self::Or,
        }
    }
}

impl From<UnaryOp> for TokenKind {
    #[inline]
    fn from(op: UnaryOp) -> Self {
        match op {
            UnaryOp::Invert => Self::Tilde,
            UnaryOp::Not => Self::Not,
            UnaryOp::UAdd => Self::Plus,
            UnaryOp::USub => Self::Minus,
        }
    }
}

impl From<Operator> for TokenKind {
    #[inline]
    fn from(op: Operator) -> Self {
        match op {
            Operator::Add => Self::Plus,
            Operator::Sub => Self::Minus,
            Operator::Mult => Self::Star,
            Operator::MatMult => Self::At,
            Operator::Div => Self::Slash,
            Operator::Mod => Self::Percent,
            Operator::Pow => Self::DoubleStar,
            Operator::LShift => Self::LeftShift,
            Operator::RShift => Self::RightShift,
            Operator::BitOr => Self::Vbar,
            Operator::BitXor => Self::CircumFlex,
            Operator::BitAnd => Self::Amper,
            Operator::FloorDiv => Self::DoubleSlash,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Unknown => "Unknown",
            Self::Newline => "newline",
            Self::NonLogicalNewline => "NonLogicalNewline",
            Self::Indent => "indent",
            Self::Dedent => "dedent",
            Self::EndOfFile => "end of file",
            Self::Name => "name",
            Self::Int => "int",
            Self::Float => "float",
            Self::Complex => "complex",
            Self::String => "string",
            Self::FStringStart => "FStringStart",
            Self::FStringMiddle => "FStringMiddle",
            Self::FStringEnd => "FStringEnd",
            Self::TStringStart => "TStringStart",
            Self::TStringMiddle => "TStringMiddle",
            Self::TStringEnd => "TStringEnd",
            Self::IpyEscapeCommand => "IPython escape command",
            Self::Comment => "comment",
            Self::Question => "'?'",
            Self::Exclamation => "'!'",
            Self::Lpar => "'('",
            Self::Rpar => "')'",
            Self::Lsqb => "'['",
            Self::Rsqb => "']'",
            Self::Lbrace => "'{'",
            Self::Rbrace => "'}'",
            Self::Equal => "'='",
            Self::ColonEqual => "':='",
            Self::Dot => "'.'",
            Self::Colon => "':'",
            Self::Semi => "';'",
            Self::Comma => "','",
            Self::Rarrow => "'->'",
            Self::Plus => "'+'",
            Self::Minus => "'-'",
            Self::Star => "'*'",
            Self::DoubleStar => "'**'",
            Self::Slash => "'/'",
            Self::DoubleSlash => "'//'",
            Self::Percent => "'%'",
            Self::Vbar => "'|'",
            Self::Amper => "'&'",
            Self::CircumFlex => "'^'",
            Self::LeftShift => "'<<'",
            Self::RightShift => "'>>'",
            Self::Tilde => "'~'",
            Self::At => "'@'",
            Self::Less => "'<'",
            Self::Greater => "'>'",
            Self::EqEqual => "'=='",
            Self::NotEqual => "'!='",
            Self::LessEqual => "'<='",
            Self::GreaterEqual => "'>='",
            Self::PlusEqual => "'+='",
            Self::MinusEqual => "'-='",
            Self::StarEqual => "'*='",
            Self::DoubleStarEqual => "'**='",
            Self::SlashEqual => "'/='",
            Self::DoubleSlashEqual => "'//='",
            Self::PercentEqual => "'%='",
            Self::VbarEqual => "'|='",
            Self::AmperEqual => "'&='",
            Self::CircumflexEqual => "'^='",
            Self::LeftShiftEqual => "'<<='",
            Self::RightShiftEqual => "'>>='",
            Self::AtEqual => "'@='",
            Self::Ellipsis => "'...'",
            Self::False => "'False'",
            Self::None => "'None'",
            Self::True => "'True'",
            Self::And => "'and'",
            Self::As => "'as'",
            Self::Assert => "'assert'",
            Self::Async => "'async'",
            Self::Await => "'await'",
            Self::Break => "'break'",
            Self::Class => "'class'",
            Self::Continue => "'continue'",
            Self::Def => "'def'",
            Self::Del => "'del'",
            Self::Elif => "'elif'",
            Self::Else => "'else'",
            Self::Except => "'except'",
            Self::Finally => "'finally'",
            Self::For => "'for'",
            Self::From => "'from'",
            Self::Global => "'global'",
            Self::If => "'if'",
            Self::Import => "'import'",
            Self::In => "'in'",
            Self::Is => "'is'",
            Self::Lambda => "'lambda'",
            Self::Nonlocal => "'nonlocal'",
            Self::Not => "'not'",
            Self::Or => "'or'",
            Self::Pass => "'pass'",
            Self::Raise => "'raise'",
            Self::Return => "'return'",
            Self::Try => "'try'",
            Self::While => "'while'",
            Self::Match => "'match'",
            Self::Type => "'type'",
            Self::Case => "'case'",
            Self::With => "'with'",
            Self::Yield => "'yield'",
        };
        f.write_str(value)
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct TokenFlags: u8 {
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

        /// The token is a raw string i.e., prefixed with `r` or `R`
        const RAW_STRING = Self::RAW_STRING_LOWERCASE.bits() | Self::RAW_STRING_UPPERCASE.bits();
    }
}

impl get_size2::GetSize for TokenFlags {}

impl StringFlags for TokenFlags {
    fn quote_style(self) -> Quote {
        if self.intersects(Self::DOUBLE_QUOTES) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn triple_quotes(self) -> TripleQuotes {
        if self.intersects(Self::TRIPLE_QUOTED_STRING) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        if self.intersects(Self::F_STRING) {
            if self.intersects(Self::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(Self::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Format(FStringPrefix::Regular)
            }
        } else if self.intersects(Self::T_STRING) {
            if self.intersects(Self::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(Self::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Template(TStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Template(TStringPrefix::Regular)
            }
        } else if self.intersects(Self::BYTE_STRING) {
            if self.intersects(Self::RAW_STRING_LOWERCASE) {
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false })
            } else if self.intersects(Self::RAW_STRING_UPPERCASE) {
                AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true })
            } else {
                AnyStringPrefix::Bytes(ByteStringPrefix::Regular)
            }
        } else if self.intersects(Self::RAW_STRING_LOWERCASE) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false })
        } else if self.intersects(Self::RAW_STRING_UPPERCASE) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true })
        } else if self.intersects(Self::UNICODE_STRING) {
            AnyStringPrefix::Regular(StringLiteralPrefix::Unicode)
        } else {
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
        }
    }
}

impl TokenFlags {
    /// Returns `true` if the token is an f-string.
    pub(crate) const fn is_f_string(self) -> bool {
        self.intersects(Self::F_STRING)
    }

    /// Returns `true` if the token is a t-string.
    pub(crate) const fn is_t_string(self) -> bool {
        self.intersects(Self::T_STRING)
    }

    /// Returns `true` if the token is a t-string.
    pub(crate) const fn is_interpolated_string(self) -> bool {
        self.intersects(Self::T_STRING.union(Self::F_STRING))
    }

    /// Returns `true` if the token is a triple-quoted t-string.
    pub(crate) fn is_triple_quoted_interpolated_string(self) -> bool {
        self.intersects(Self::TRIPLE_QUOTED_STRING) && self.is_interpolated_string()
    }

    /// Returns `true` if the token is a raw string.
    pub(crate) const fn is_raw_string(self) -> bool {
        self.intersects(Self::RAW_STRING)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) enum TokenValue {
    #[default]
    None,
    /// Token value for a name, commonly known as an identifier.
    ///
    /// Unicode names are NFKC-normalized by the lexer,
    /// matching [the behaviour of Python's lexer](https://docs.python.org/3/reference/lexical_analysis.html#identifiers)
    Name(Name),
    /// Token value for an integer.
    Int(Int),
    /// Token value for a floating point number.
    Float(f64),
    /// Token value for a complex number.
    Complex {
        /// The real part of the complex number.
        real: f64,
        /// The imaginary part of the complex number.
        imag: f64,
    },
    /// Token value for a string.
    String(Box<str>),
    /// Token value that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    InterpolatedStringMiddle(Box<str>),
    /// Token value for IPython escape commands. These are recognized by the lexer
    /// only when the mode is [`Mode::Ipython`].
    IpyEscapeCommand {
        /// The magic command value.
        value: Box<str>,
        /// The kind of magic command.
        kind: IpyEscapeKind,
    },
}
