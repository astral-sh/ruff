//! Token type for Python source code created by the lexer and consumed by the `ruff_python_parser`.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Grammar/Tokens

use ruff_python_ast::{AnyStringKind, BoolOp, Int, IpyEscapeKind, Operator, UnaryOp};
use std::fmt;

use crate::Mode;

/// The set of tokens the Python source code can be tokenized in.
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Tok {
    /// Token value for a name, commonly known as an identifier.
    Name {
        /// The name value.
        ///
        /// Unicode names are NFKC-normalized by the lexer,
        /// matching [the behaviour of Python's lexer](https://docs.python.org/3/reference/lexical_analysis.html#identifiers)
        name: Box<str>,
    },
    /// Token value for an integer.
    Int {
        /// The integer value.
        value: Int,
    },
    /// Token value for a floating point number.
    Float {
        /// The float value.
        value: f64,
    },
    /// Token value for a complex number.
    Complex {
        /// The real part of the complex number.
        real: f64,
        /// The imaginary part of the complex number.
        imag: f64,
    },
    /// Token value for a string.
    String {
        /// The string value.
        value: Box<str>,
        /// Flags that can be queried to determine the quote style
        /// and prefixes of the string
        kind: AnyStringKind,
    },
    /// Token value for the start of an f-string. This includes the `f`/`F`/`fr` prefix
    /// and the opening quote(s).
    FStringStart(AnyStringKind),
    /// Token value that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    FStringMiddle {
        /// The string value.
        value: Box<str>,
        /// Flags that can be queried to determine the quote style
        /// and prefixes of the string
        kind: AnyStringKind,
    },
    /// Token value for the end of an f-string. This includes the closing quote.
    FStringEnd,
    /// Token value for IPython escape commands. These are recognized by the lexer
    /// only when the mode is [`Mode::Ipython`].
    IpyEscapeCommand {
        /// The magic command value.
        value: Box<str>,
        /// The kind of magic command.
        kind: IpyEscapeKind,
    },
    /// Token value for a comment. These are filtered out of the token stream prior to parsing.
    Comment(Box<str>),
    /// Token value for a newline.
    Newline,
    /// Token value for a newline that is not a logical line break. These are filtered out of
    /// the token stream prior to parsing.
    NonLogicalNewline,
    /// Token value for an indent.
    Indent,
    /// Token value for a dedent.
    Dedent,
    EndOfFile,
    /// Token value for a question mark `?`. This is only used in [`Mode::Ipython`].
    Question,
    /// Token value for a exclamation mark `!`.
    Exclamation,
    /// Token value for a left parenthesis `(`.
    Lpar,
    /// Token value for a right parenthesis `)`.
    Rpar,
    /// Token value for a left square bracket `[`.
    Lsqb,
    /// Token value for a right square bracket `]`.
    Rsqb,
    /// Token value for a colon `:`.
    Colon,
    /// Token value for a comma `,`.
    Comma,
    /// Token value for a semicolon `;`.
    Semi,
    /// Token value for plus `+`.
    Plus,
    /// Token value for minus `-`.
    Minus,
    /// Token value for star `*`.
    Star,
    /// Token value for slash `/`.
    Slash,
    /// Token value for vertical bar `|`.
    Vbar,
    /// Token value for ampersand `&`.
    Amper,
    /// Token value for less than `<`.
    Less,
    /// Token value for greater than `>`.
    Greater,
    /// Token value for equal `=`.
    Equal,
    /// Token value for dot `.`.
    Dot,
    /// Token value for percent `%`.
    Percent,
    /// Token value for left bracket `{`.
    Lbrace,
    /// Token value for right bracket `}`.
    Rbrace,
    /// Token value for double equal `==`.
    EqEqual,
    /// Token value for not equal `!=`.
    NotEqual,
    /// Token value for less than or equal `<=`.
    LessEqual,
    /// Token value for greater than or equal `>=`.
    GreaterEqual,
    /// Token value for tilde `~`.
    Tilde,
    /// Token value for caret `^`.
    CircumFlex,
    /// Token value for left shift `<<`.
    LeftShift,
    /// Token value for right shift `>>`.
    RightShift,
    /// Token value for double star `**`.
    DoubleStar,
    /// Token value for double star equal `**=`.
    DoubleStarEqual,
    /// Token value for plus equal `+=`.
    PlusEqual,
    /// Token value for minus equal `-=`.
    MinusEqual,
    /// Token value for star equal `*=`.
    StarEqual,
    /// Token value for slash equal `/=`.
    SlashEqual,
    /// Token value for percent equal `%=`.
    PercentEqual,
    /// Token value for ampersand equal `&=`.
    AmperEqual,
    /// Token value for vertical bar equal `|=`.
    VbarEqual,
    /// Token value for caret equal `^=`.
    CircumflexEqual,
    /// Token value for left shift equal `<<=`.
    LeftShiftEqual,
    /// Token value for right shift equal `>>=`.
    RightShiftEqual,
    /// Token value for double slash `//`.
    DoubleSlash,
    /// Token value for double slash equal `//=`.
    DoubleSlashEqual,
    /// Token value for colon equal `:=`.
    ColonEqual,
    /// Token value for at `@`.
    At,
    /// Token value for at equal `@=`.
    AtEqual,
    /// Token value for arrow `->`.
    Rarrow,
    /// Token value for ellipsis `...`.
    Ellipsis,

    // Self documenting.
    // Keywords (alphabetically):
    False,
    None,
    True,

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
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    Match,
    Type,
    Case,
    With,
    Yield,

    Unknown,
    // RustPython specific.
    StartModule,
    StartExpression,
}

impl Tok {
    pub fn start_marker(mode: Mode) -> Self {
        match mode {
            Mode::Module | Mode::Ipython => Tok::StartModule,
            Mode::Expression => Tok::StartExpression,
        }
    }
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use Tok::*;
        match self {
            Name { name } => write!(f, "{name}"),
            Int { value } => write!(f, "{value}"),
            Float { value } => write!(f, "{value}"),
            Complex { real, imag } => write!(f, "{real}j{imag}"),
            String { value, kind } => {
                write!(f, "{}", kind.format_string_contents(value))
            }
            FStringStart(_) => f.write_str("FStringStart"),
            FStringMiddle { value, .. } => f.write_str(value),
            FStringEnd => f.write_str("FStringEnd"),
            IpyEscapeCommand { kind, value } => write!(f, "{kind}{value}"),
            Newline => f.write_str("Newline"),
            NonLogicalNewline => f.write_str("NonLogicalNewline"),
            Indent => f.write_str("Indent"),
            Dedent => f.write_str("Dedent"),
            StartModule => f.write_str("StartProgram"),
            StartExpression => f.write_str("StartExpression"),
            EndOfFile => f.write_str("EOF"),
            Question => f.write_str("?"),
            Exclamation => f.write_str("!"),
            Lpar => f.write_str("("),
            Rpar => f.write_str(")"),
            Lsqb => f.write_str("["),
            Rsqb => f.write_str("]"),
            Colon => f.write_str(":"),
            Comma => f.write_str(","),
            Comment(value) => f.write_str(value),
            Semi => f.write_str(";"),
            Plus => f.write_str("+"),
            Minus => f.write_str("-"),
            Star => f.write_str("*"),
            Slash => f.write_str("/"),
            Vbar => f.write_str("|"),
            Amper => f.write_str("&"),
            Less => f.write_str("<"),
            Greater => f.write_str(">"),
            Equal => f.write_str("="),
            Dot => f.write_str("."),
            Percent => f.write_str("%"),
            Lbrace => f.write_str("{"),
            Rbrace => f.write_str("}"),
            EqEqual => f.write_str("=="),
            NotEqual => f.write_str("!="),
            LessEqual => f.write_str("<="),
            GreaterEqual => f.write_str(">="),
            Tilde => f.write_str("~"),
            CircumFlex => f.write_str("^"),
            LeftShift => f.write_str("<<"),
            RightShift => f.write_str(">>"),
            DoubleStar => f.write_str("**"),
            DoubleStarEqual => f.write_str("**="),
            PlusEqual => f.write_str("+="),
            MinusEqual => f.write_str("-="),
            StarEqual => f.write_str("*="),
            SlashEqual => f.write_str("/="),
            PercentEqual => f.write_str("%="),
            AmperEqual => f.write_str("&="),
            VbarEqual => f.write_str("|="),
            CircumflexEqual => f.write_str("^="),
            LeftShiftEqual => f.write_str("<<="),
            RightShiftEqual => f.write_str(">>="),
            DoubleSlash => f.write_str("//"),
            DoubleSlashEqual => f.write_str("//="),
            At => f.write_str("@"),
            AtEqual => f.write_str("@="),
            Rarrow => f.write_str("->"),
            Ellipsis => f.write_str("..."),
            False => f.write_str("False"),
            None => f.write_str("None"),
            True => f.write_str("True"),
            And => f.write_str("and"),
            As => f.write_str("as"),
            Assert => f.write_str("assert"),
            Async => f.write_str("async"),
            Await => f.write_str("await"),
            Break => f.write_str("break"),
            Class => f.write_str("class"),
            Continue => f.write_str("continue"),
            Def => f.write_str("def"),
            Del => f.write_str("del"),
            Elif => f.write_str("elif"),
            Else => f.write_str("else"),
            Except => f.write_str("except"),
            Finally => f.write_str("finally"),
            For => f.write_str("for"),
            From => f.write_str("from"),
            Global => f.write_str("global"),
            If => f.write_str("if"),
            Import => f.write_str("import"),
            In => f.write_str("in"),
            Is => f.write_str("is"),
            Lambda => f.write_str("lambda"),
            Nonlocal => f.write_str("nonlocal"),
            Not => f.write_str("not"),
            Or => f.write_str("or"),
            Pass => f.write_str("pass"),
            Raise => f.write_str("raise"),
            Return => f.write_str("return"),
            Try => f.write_str("try"),
            While => f.write_str("while"),
            Match => f.write_str("match"),
            Type => f.write_str("type"),
            Case => f.write_str("case"),
            With => f.write_str("with"),
            Yield => f.write_str("yield"),
            ColonEqual => f.write_str(":="),
            Unknown => f.write_str("<Unknown>>"),
        }
    }
}

/// A kind of token.
///
/// This is a lightweight representation of [`Tok`] which doesn't contain any information
/// about the token itself.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TokenKind {
    /// Token value for a name, commonly known as an identifier.
    Name,
    /// Token value for an integer.
    Int,
    /// Token value for a floating point number.
    Float,
    /// Token value for a complex number.
    Complex,
    /// Token value for a string.
    String,
    /// Token value for the start of an f-string. This includes the `f`/`F`/`fr` prefix
    /// and the opening quote(s).
    FStringStart,
    /// Token value that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    FStringMiddle,
    /// Token value for the end of an f-string. This includes the closing quote.
    FStringEnd,
    /// Token value for a IPython escape command.
    IpyEscapeCommand,
    /// Token value for a comment. These are filtered out of the token stream prior to parsing.
    Comment,
    /// Token value for a newline.
    Newline,
    /// Token value for a newline that is not a logical line break. These are filtered out of
    /// the token stream prior to parsing.
    NonLogicalNewline,
    /// Token value for an indent.
    Indent,
    /// Token value for a dedent.
    Dedent,
    EndOfFile,
    /// Token value for a question mark `?`.
    Question,
    /// Token value for an exclamation mark `!`.
    Exclamation,
    /// Token value for a left parenthesis `(`.
    Lpar,
    /// Token value for a right parenthesis `)`.
    Rpar,
    /// Token value for a left square bracket `[`.
    Lsqb,
    /// Token value for a right square bracket `]`.
    Rsqb,
    /// Token value for a colon `:`.
    Colon,
    /// Token value for a comma `,`.
    Comma,
    /// Token value for a semicolon `;`.
    Semi,
    /// Token value for plus `+`.
    Plus,
    /// Token value for minus `-`.
    Minus,
    /// Token value for star `*`.
    Star,
    /// Token value for slash `/`.
    Slash,
    /// Token value for vertical bar `|`.
    Vbar,
    /// Token value for ampersand `&`.
    Amper,
    /// Token value for less than `<`.
    Less,
    /// Token value for greater than `>`.
    Greater,
    /// Token value for equal `=`.
    Equal,
    /// Token value for dot `.`.
    Dot,
    /// Token value for percent `%`.
    Percent,
    /// Token value for left bracket `{`.
    Lbrace,
    /// Token value for right bracket `}`.
    Rbrace,
    /// Token value for double equal `==`.
    EqEqual,
    /// Token value for not equal `!=`.
    NotEqual,
    /// Token value for less than or equal `<=`.
    LessEqual,
    /// Token value for greater than or equal `>=`.
    GreaterEqual,
    /// Token value for tilde `~`.
    Tilde,
    /// Token value for caret `^`.
    CircumFlex,
    /// Token value for left shift `<<`.
    LeftShift,
    /// Token value for right shift `>>`.
    RightShift,
    /// Token value for double star `**`.
    DoubleStar,
    /// Token value for double star equal `**=`.
    DoubleStarEqual,
    /// Token value for plus equal `+=`.
    PlusEqual,
    /// Token value for minus equal `-=`.
    MinusEqual,
    /// Token value for star equal `*=`.
    StarEqual,
    /// Token value for slash equal `/=`.
    SlashEqual,
    /// Token value for percent equal `%=`.
    PercentEqual,
    /// Token value for ampersand equal `&=`.
    AmperEqual,
    /// Token value for vertical bar equal `|=`.
    VbarEqual,
    /// Token value for caret equal `^=`.
    CircumflexEqual,
    /// Token value for left shift equal `<<=`.
    LeftShiftEqual,
    /// Token value for right shift equal `>>=`.
    RightShiftEqual,
    /// Token value for double slash `//`.
    DoubleSlash,
    /// Token value for double slash equal `//=`.
    DoubleSlashEqual,
    /// Token value for colon equal `:=`.
    ColonEqual,
    /// Token value for at `@`.
    At,
    /// Token value for at equal `@=`.
    AtEqual,
    /// Token value for arrow `->`.
    Rarrow,
    /// Token value for ellipsis `...`.
    Ellipsis,

    // Self documenting.
    // Keywords (alphabetically):
    False,
    None,
    True,

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
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    Match,
    Type,
    Case,
    With,
    Yield,

    Unknown,
    // RustPython specific.
    StartModule,
    StartInteractive,
    StartExpression,
}

impl TokenKind {
    #[inline]
    pub const fn is_newline(self) -> bool {
        matches!(self, TokenKind::Newline | TokenKind::NonLogicalNewline)
    }

    #[inline]
    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            TokenKind::False
                | TokenKind::True
                | TokenKind::None
                | TokenKind::And
                | TokenKind::As
                | TokenKind::Assert
                | TokenKind::Await
                | TokenKind::Break
                | TokenKind::Class
                | TokenKind::Continue
                | TokenKind::Def
                | TokenKind::Del
                | TokenKind::Elif
                | TokenKind::Else
                | TokenKind::Except
                | TokenKind::Finally
                | TokenKind::For
                | TokenKind::From
                | TokenKind::Global
                | TokenKind::If
                | TokenKind::Import
                | TokenKind::In
                | TokenKind::Is
                | TokenKind::Lambda
                | TokenKind::Nonlocal
                | TokenKind::Not
                | TokenKind::Or
                | TokenKind::Pass
                | TokenKind::Raise
                | TokenKind::Return
                | TokenKind::Try
                | TokenKind::While
                | TokenKind::With
                | TokenKind::Yield
        )
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

    #[inline]
    pub const fn is_singleton(self) -> bool {
        matches!(self, TokenKind::False | TokenKind::True | TokenKind::None)
    }

    #[inline]
    pub const fn is_trivia(&self) -> bool {
        matches!(
            self,
            TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::NonLogicalNewline
                | TokenKind::Comment
        )
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

    #[inline]
    pub const fn is_soft_keyword(self) -> bool {
        matches!(self, TokenKind::Match | TokenKind::Case)
    }

    /// Returns `true` if the current token is a unary arithmetic operator.
    #[inline]
    pub const fn is_unary_arithmetic_operator(self) -> bool {
        matches!(self, TokenKind::Plus | TokenKind::Minus)
    }

    /// Returns the [`UnaryOp`] that corresponds to this token kind, if it is an arithmetic unary
    /// operator, otherwise return [None].
    ///
    /// Use [`TokenKind::as_unary_operator`] to match against any unary operator.
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
    /// Use [`TokenKind::as_unary_arithmetic_operator`] to match against only an arithmetic unary
    /// operator.
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
    /// Use [`TokenKind::as_augmented_assign_operator`] to match against an augmented assignment
    /// token.
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

    pub const fn from_token(token: &Tok) -> Self {
        match token {
            Tok::Name { .. } => TokenKind::Name,
            Tok::Int { .. } => TokenKind::Int,
            Tok::Float { .. } => TokenKind::Float,
            Tok::Complex { .. } => TokenKind::Complex,
            Tok::String { .. } => TokenKind::String,
            Tok::FStringStart(_) => TokenKind::FStringStart,
            Tok::FStringMiddle { .. } => TokenKind::FStringMiddle,
            Tok::FStringEnd => TokenKind::FStringEnd,
            Tok::IpyEscapeCommand { .. } => TokenKind::IpyEscapeCommand,
            Tok::Comment(_) => TokenKind::Comment,
            Tok::Newline => TokenKind::Newline,
            Tok::NonLogicalNewline => TokenKind::NonLogicalNewline,
            Tok::Indent => TokenKind::Indent,
            Tok::Dedent => TokenKind::Dedent,
            Tok::EndOfFile => TokenKind::EndOfFile,
            Tok::Question => TokenKind::Question,
            Tok::Exclamation => TokenKind::Exclamation,
            Tok::Lpar => TokenKind::Lpar,
            Tok::Rpar => TokenKind::Rpar,
            Tok::Lsqb => TokenKind::Lsqb,
            Tok::Rsqb => TokenKind::Rsqb,
            Tok::Colon => TokenKind::Colon,
            Tok::Comma => TokenKind::Comma,
            Tok::Semi => TokenKind::Semi,
            Tok::Plus => TokenKind::Plus,
            Tok::Minus => TokenKind::Minus,
            Tok::Star => TokenKind::Star,
            Tok::Slash => TokenKind::Slash,
            Tok::Vbar => TokenKind::Vbar,
            Tok::Amper => TokenKind::Amper,
            Tok::Less => TokenKind::Less,
            Tok::Greater => TokenKind::Greater,
            Tok::Equal => TokenKind::Equal,
            Tok::Dot => TokenKind::Dot,
            Tok::Percent => TokenKind::Percent,
            Tok::Lbrace => TokenKind::Lbrace,
            Tok::Rbrace => TokenKind::Rbrace,
            Tok::EqEqual => TokenKind::EqEqual,
            Tok::NotEqual => TokenKind::NotEqual,
            Tok::LessEqual => TokenKind::LessEqual,
            Tok::GreaterEqual => TokenKind::GreaterEqual,
            Tok::Tilde => TokenKind::Tilde,
            Tok::CircumFlex => TokenKind::CircumFlex,
            Tok::LeftShift => TokenKind::LeftShift,
            Tok::RightShift => TokenKind::RightShift,
            Tok::DoubleStar => TokenKind::DoubleStar,
            Tok::DoubleStarEqual => TokenKind::DoubleStarEqual,
            Tok::PlusEqual => TokenKind::PlusEqual,
            Tok::MinusEqual => TokenKind::MinusEqual,
            Tok::StarEqual => TokenKind::StarEqual,
            Tok::SlashEqual => TokenKind::SlashEqual,
            Tok::PercentEqual => TokenKind::PercentEqual,
            Tok::AmperEqual => TokenKind::AmperEqual,
            Tok::VbarEqual => TokenKind::VbarEqual,
            Tok::CircumflexEqual => TokenKind::CircumflexEqual,
            Tok::LeftShiftEqual => TokenKind::LeftShiftEqual,
            Tok::RightShiftEqual => TokenKind::RightShiftEqual,
            Tok::DoubleSlash => TokenKind::DoubleSlash,
            Tok::DoubleSlashEqual => TokenKind::DoubleSlashEqual,
            Tok::ColonEqual => TokenKind::ColonEqual,
            Tok::At => TokenKind::At,
            Tok::AtEqual => TokenKind::AtEqual,
            Tok::Rarrow => TokenKind::Rarrow,
            Tok::Ellipsis => TokenKind::Ellipsis,
            Tok::False => TokenKind::False,
            Tok::None => TokenKind::None,
            Tok::True => TokenKind::True,
            Tok::And => TokenKind::And,
            Tok::As => TokenKind::As,
            Tok::Assert => TokenKind::Assert,
            Tok::Async => TokenKind::Async,
            Tok::Await => TokenKind::Await,
            Tok::Break => TokenKind::Break,
            Tok::Class => TokenKind::Class,
            Tok::Continue => TokenKind::Continue,
            Tok::Def => TokenKind::Def,
            Tok::Del => TokenKind::Del,
            Tok::Elif => TokenKind::Elif,
            Tok::Else => TokenKind::Else,
            Tok::Except => TokenKind::Except,
            Tok::Finally => TokenKind::Finally,
            Tok::For => TokenKind::For,
            Tok::From => TokenKind::From,
            Tok::Global => TokenKind::Global,
            Tok::If => TokenKind::If,
            Tok::Import => TokenKind::Import,
            Tok::In => TokenKind::In,
            Tok::Is => TokenKind::Is,
            Tok::Lambda => TokenKind::Lambda,
            Tok::Nonlocal => TokenKind::Nonlocal,
            Tok::Not => TokenKind::Not,
            Tok::Or => TokenKind::Or,
            Tok::Pass => TokenKind::Pass,
            Tok::Raise => TokenKind::Raise,
            Tok::Return => TokenKind::Return,
            Tok::Try => TokenKind::Try,
            Tok::While => TokenKind::While,
            Tok::Match => TokenKind::Match,
            Tok::Case => TokenKind::Case,
            Tok::Type => TokenKind::Type,
            Tok::With => TokenKind::With,
            Tok::Yield => TokenKind::Yield,
            Tok::Unknown => TokenKind::Unknown,
            Tok::StartModule => TokenKind::StartModule,
            Tok::StartExpression => TokenKind::StartExpression,
        }
    }
}

impl From<&Tok> for TokenKind {
    fn from(value: &Tok) -> Self {
        Self::from_token(value)
    }
}

impl From<Tok> for TokenKind {
    fn from(value: Tok) -> Self {
        Self::from_token(&value)
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
            TokenKind::StartModule => "StartModule",
            TokenKind::StartExpression => "StartExpression",
            TokenKind::StartInteractive => "StartInteractive",
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
    use crate::Tok;
    use static_assertions::assert_eq_size;

    assert_eq_size!(Tok, [u8; 24]);
    assert_eq_size!(LexicalErrorType, [u8; 24]);
    assert_eq_size!(Result<Tok, LexicalError>, [u8; 32]);
}
