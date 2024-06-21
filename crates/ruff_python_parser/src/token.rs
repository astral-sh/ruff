//! Token kinds for Python source code created by the lexer and consumed by the `ruff_python_parser`.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Grammar/Tokens

use std::fmt;

use ruff_python_ast::{BoolOp, Operator, UnaryOp};

/// A kind of a token.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
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

    /// Returns the [`UnaryOp`] that corresponds to this token kind, if it is a unary arithmetic
    /// operator, otherwise return [None].
    ///
    /// Use [`as_unary_operator`] to match against any unary operator.
    ///
    /// [`as_unary_operator`]: TokenKind::as_unary_operator
    #[inline]
    pub(crate) const fn as_unary_arithmetic_operator(self) -> Option<UnaryOp> {
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
    pub(crate) const fn as_unary_operator(self) -> Option<UnaryOp> {
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
    pub(crate) const fn as_bool_operator(self) -> Option<BoolOp> {
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
    pub(crate) const fn as_binary_operator(self) -> Option<Operator> {
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
    pub(crate) const fn as_augmented_assign_operator(self) -> Option<Operator> {
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
            TokenKind::IpyEscapeCommand => "IPython escape command",
            TokenKind::Comment => "comment",
            TokenKind::Question => "'?'",
            TokenKind::Exclamation => "'!'",
            TokenKind::Lpar => "'('",
            TokenKind::Rpar => "')'",
            TokenKind::Lsqb => "'['",
            TokenKind::Rsqb => "']'",
            TokenKind::Lbrace => "'{'",
            TokenKind::Rbrace => "'}'",
            TokenKind::Equal => "'='",
            TokenKind::ColonEqual => "':='",
            TokenKind::Dot => "'.'",
            TokenKind::Colon => "':'",
            TokenKind::Semi => "';'",
            TokenKind::Comma => "','",
            TokenKind::Rarrow => "'->'",
            TokenKind::Plus => "'+'",
            TokenKind::Minus => "'-'",
            TokenKind::Star => "'*'",
            TokenKind::DoubleStar => "'**'",
            TokenKind::Slash => "'/'",
            TokenKind::DoubleSlash => "'//'",
            TokenKind::Percent => "'%'",
            TokenKind::Vbar => "'|'",
            TokenKind::Amper => "'&'",
            TokenKind::CircumFlex => "'^'",
            TokenKind::LeftShift => "'<<'",
            TokenKind::RightShift => "'>>'",
            TokenKind::Tilde => "'~'",
            TokenKind::At => "'@'",
            TokenKind::Less => "'<'",
            TokenKind::Greater => "'>'",
            TokenKind::EqEqual => "'=='",
            TokenKind::NotEqual => "'!='",
            TokenKind::LessEqual => "'<='",
            TokenKind::GreaterEqual => "'>='",
            TokenKind::PlusEqual => "'+='",
            TokenKind::MinusEqual => "'-='",
            TokenKind::StarEqual => "'*='",
            TokenKind::DoubleStarEqual => "'**='",
            TokenKind::SlashEqual => "'/='",
            TokenKind::DoubleSlashEqual => "'//='",
            TokenKind::PercentEqual => "'%='",
            TokenKind::VbarEqual => "'|='",
            TokenKind::AmperEqual => "'&='",
            TokenKind::CircumflexEqual => "'^='",
            TokenKind::LeftShiftEqual => "'<<='",
            TokenKind::RightShiftEqual => "'>>='",
            TokenKind::AtEqual => "'@='",
            TokenKind::Ellipsis => "'...'",
            TokenKind::False => "'False'",
            TokenKind::None => "'None'",
            TokenKind::True => "'True'",
            TokenKind::And => "'and'",
            TokenKind::As => "'as'",
            TokenKind::Assert => "'assert'",
            TokenKind::Async => "'async'",
            TokenKind::Await => "'await'",
            TokenKind::Break => "'break'",
            TokenKind::Class => "'class'",
            TokenKind::Continue => "'continue'",
            TokenKind::Def => "'def'",
            TokenKind::Del => "'del'",
            TokenKind::Elif => "'elif'",
            TokenKind::Else => "'else'",
            TokenKind::Except => "'except'",
            TokenKind::Finally => "'finally'",
            TokenKind::For => "'for'",
            TokenKind::From => "'from'",
            TokenKind::Global => "'global'",
            TokenKind::If => "'if'",
            TokenKind::Import => "'import'",
            TokenKind::In => "'in'",
            TokenKind::Is => "'is'",
            TokenKind::Lambda => "'lambda'",
            TokenKind::Nonlocal => "'nonlocal'",
            TokenKind::Not => "'not'",
            TokenKind::Or => "'or'",
            TokenKind::Pass => "'pass'",
            TokenKind::Raise => "'raise'",
            TokenKind::Return => "'return'",
            TokenKind::Try => "'try'",
            TokenKind::While => "'while'",
            TokenKind::Match => "'match'",
            TokenKind::Type => "'type'",
            TokenKind::Case => "'case'",
            TokenKind::With => "'with'",
            TokenKind::Yield => "'yield'",
        };
        f.write_str(value)
    }
}

#[cfg(target_pointer_width = "64")]
mod sizes {
    use crate::lexer::{LexicalError, LexicalErrorType};
    use static_assertions::assert_eq_size;

    assert_eq_size!(LexicalErrorType, [u8; 24]);
    assert_eq_size!(LexicalError, [u8; 32]);
}
