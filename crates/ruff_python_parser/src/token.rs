//! Token type for Python source code created by the lexer and consumed by the `ruff_python_parser`.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Include/internal/pycore_token.h;
use crate::Mode;
use num_bigint::BigInt;
use ruff_python_ast::IpyEscapeKind;
use ruff_text_size::TextSize;
use std::fmt;

/// The set of tokens the Python source code can be tokenized in.
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Tok {
    /// Token value for a name, commonly known as an identifier.
    Name {
        /// The name value.
        name: String,
    },
    /// Token value for an integer.
    Int {
        /// The integer value.
        value: BigInt,
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
        value: String,
        /// The kind of string.
        kind: StringKind,
        /// Whether the string is triple quoted.
        triple_quoted: bool,
    },
    /// Token value for IPython escape commands. These are recognized by the lexer
    /// only when the mode is [`Mode::Ipython`].
    IpyEscapeCommand {
        /// The magic command value.
        value: String,
        /// The kind of magic command.
        kind: IpyEscapeKind,
    },
    /// Token value for a comment. These are filtered out of the token stream prior to parsing.
    Comment(String),
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
            Name { name } => write!(f, "'{name}'"),
            Int { value } => write!(f, "'{value}'"),
            Float { value } => write!(f, "'{value}'"),
            Complex { real, imag } => write!(f, "{real}j{imag}"),
            String {
                value,
                kind,
                triple_quoted,
            } => {
                let quotes = "\"".repeat(if *triple_quoted { 3 } else { 1 });
                write!(f, "{kind}{quotes}{value}{quotes}")
            }
            IpyEscapeCommand { kind, value } => write!(f, "{kind}{value}"),
            Newline => f.write_str("Newline"),
            NonLogicalNewline => f.write_str("NonLogicalNewline"),
            Indent => f.write_str("Indent"),
            Dedent => f.write_str("Dedent"),
            StartModule => f.write_str("StartProgram"),
            StartExpression => f.write_str("StartExpression"),
            EndOfFile => f.write_str("EOF"),
            Question => f.write_str("'?'"),
            Lpar => f.write_str("'('"),
            Rpar => f.write_str("')'"),
            Lsqb => f.write_str("'['"),
            Rsqb => f.write_str("']'"),
            Colon => f.write_str("':'"),
            Comma => f.write_str("','"),
            Comment(value) => f.write_str(value),
            Semi => f.write_str("';'"),
            Plus => f.write_str("'+'"),
            Minus => f.write_str("'-'"),
            Star => f.write_str("'*'"),
            Slash => f.write_str("'/'"),
            Vbar => f.write_str("'|'"),
            Amper => f.write_str("'&'"),
            Less => f.write_str("'<'"),
            Greater => f.write_str("'>'"),
            Equal => f.write_str("'='"),
            Dot => f.write_str("'.'"),
            Percent => f.write_str("'%'"),
            Lbrace => f.write_str("'{'"),
            Rbrace => f.write_str("'}'"),
            EqEqual => f.write_str("'=='"),
            NotEqual => f.write_str("'!='"),
            LessEqual => f.write_str("'<='"),
            GreaterEqual => f.write_str("'>='"),
            Tilde => f.write_str("'~'"),
            CircumFlex => f.write_str("'^'"),
            LeftShift => f.write_str("'<<'"),
            RightShift => f.write_str("'>>'"),
            DoubleStar => f.write_str("'**'"),
            DoubleStarEqual => f.write_str("'**='"),
            PlusEqual => f.write_str("'+='"),
            MinusEqual => f.write_str("'-='"),
            StarEqual => f.write_str("'*='"),
            SlashEqual => f.write_str("'/='"),
            PercentEqual => f.write_str("'%='"),
            AmperEqual => f.write_str("'&='"),
            VbarEqual => f.write_str("'|='"),
            CircumflexEqual => f.write_str("'^='"),
            LeftShiftEqual => f.write_str("'<<='"),
            RightShiftEqual => f.write_str("'>>='"),
            DoubleSlash => f.write_str("'//'"),
            DoubleSlashEqual => f.write_str("'//='"),
            At => f.write_str("'@'"),
            AtEqual => f.write_str("'@='"),
            Rarrow => f.write_str("'->'"),
            Ellipsis => f.write_str("'...'"),
            False => f.write_str("'False'"),
            None => f.write_str("'None'"),
            True => f.write_str("'True'"),
            And => f.write_str("'and'"),
            As => f.write_str("'as'"),
            Assert => f.write_str("'assert'"),
            Async => f.write_str("'async'"),
            Await => f.write_str("'await'"),
            Break => f.write_str("'break'"),
            Class => f.write_str("'class'"),
            Continue => f.write_str("'continue'"),
            Def => f.write_str("'def'"),
            Del => f.write_str("'del'"),
            Elif => f.write_str("'elif'"),
            Else => f.write_str("'else'"),
            Except => f.write_str("'except'"),
            Finally => f.write_str("'finally'"),
            For => f.write_str("'for'"),
            From => f.write_str("'from'"),
            Global => f.write_str("'global'"),
            If => f.write_str("'if'"),
            Import => f.write_str("'import'"),
            In => f.write_str("'in'"),
            Is => f.write_str("'is'"),
            Lambda => f.write_str("'lambda'"),
            Nonlocal => f.write_str("'nonlocal'"),
            Not => f.write_str("'not'"),
            Or => f.write_str("'or'"),
            Pass => f.write_str("'pass'"),
            Raise => f.write_str("'raise'"),
            Return => f.write_str("'return'"),
            Try => f.write_str("'try'"),
            While => f.write_str("'while'"),
            Match => f.write_str("'match'"),
            Type => f.write_str("'type'"),
            Case => f.write_str("'case'"),
            With => f.write_str("'with'"),
            Yield => f.write_str("'yield'"),
            ColonEqual => f.write_str("':='"),
        }
    }
}

/// The kind of string literal as described in the [String and Bytes literals]
/// section of the Python reference.
///
/// [String and Bytes literals]: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)] // TODO: is_macro::Is
pub enum StringKind {
    /// A normal string literal with no prefix.
    String,
    /// A f-string literal, with a `f` or `F` prefix.
    FString,
    /// A byte string literal, with a `b` or `B` prefix.
    Bytes,
    /// A raw string literal, with a `r` or `R` prefix.
    RawString,
    /// A raw f-string literal, with a `rf`/`fr` or `rF`/`Fr` or `Rf`/`fR` or `RF`/`FR` prefix.
    RawFString,
    /// A raw byte string literal, with a `rb`/`br` or `rB`/`Br` or `Rb`/`bR` or `RB`/`BR` prefix.
    RawBytes,
    /// A unicode string literal, with a `u` or `U` prefix.
    Unicode,
}

impl TryFrom<char> for StringKind {
    type Error = String;

    fn try_from(ch: char) -> Result<Self, String> {
        match ch {
            'r' | 'R' => Ok(StringKind::RawString),
            'f' | 'F' => Ok(StringKind::FString),
            'u' | 'U' => Ok(StringKind::Unicode),
            'b' | 'B' => Ok(StringKind::Bytes),
            c => Err(format!("Unexpected string prefix: {c}")),
        }
    }
}

impl TryFrom<[char; 2]> for StringKind {
    type Error = String;

    fn try_from(chars: [char; 2]) -> Result<Self, String> {
        match chars {
            ['r' | 'R', 'f' | 'F'] => Ok(StringKind::RawFString),
            ['f' | 'F', 'r' | 'R'] => Ok(StringKind::RawFString),
            ['r' | 'R', 'b' | 'B'] => Ok(StringKind::RawBytes),
            ['b' | 'B', 'r' | 'R'] => Ok(StringKind::RawBytes),
            [c1, c2] => Err(format!("Unexpected string prefix: {c1}{c2}")),
        }
    }
}

impl fmt::Display for StringKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use StringKind::{Bytes, FString, RawBytes, RawFString, RawString, String, Unicode};
        match self {
            String => f.write_str(""),
            FString => f.write_str("f"),
            Bytes => f.write_str("b"),
            RawString => f.write_str("r"),
            RawFString => f.write_str("rf"),
            RawBytes => f.write_str("rb"),
            Unicode => f.write_str("u"),
        }
    }
}

impl StringKind {
    /// Returns true if the string is a raw string, i,e one of
    /// [`StringKind::RawString`] or [`StringKind::RawFString`] or [`StringKind::RawBytes`].
    pub fn is_raw(&self) -> bool {
        use StringKind::{RawBytes, RawFString, RawString};
        matches!(self, RawString | RawFString | RawBytes)
    }

    /// Returns true if the string is an f-string, i,e one of
    /// [`StringKind::FString`] or [`StringKind::RawFString`].
    pub fn is_any_fstring(&self) -> bool {
        use StringKind::{FString, RawFString};
        matches!(self, FString | RawFString)
    }

    /// Returns true if the string is a byte string, i,e one of
    /// [`StringKind::Bytes`] or [`StringKind::RawBytes`].
    pub fn is_any_bytes(&self) -> bool {
        use StringKind::{Bytes, RawBytes};
        matches!(self, Bytes | RawBytes)
    }

    /// Returns true if the string is a unicode string, i,e [`StringKind::Unicode`].
    pub fn is_unicode(&self) -> bool {
        matches!(self, StringKind::Unicode)
    }

    /// Returns the number of characters in the prefix.
    pub fn prefix_len(&self) -> TextSize {
        use StringKind::{Bytes, FString, RawBytes, RawFString, RawString, String, Unicode};
        let len = match self {
            String => 0,
            RawString | FString | Unicode | Bytes => 1,
            RawFString | RawBytes => 2,
        };
        len.into()
    }
}

// TODO move to ruff_python_parser?
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
    /// Token value for a IPython escape command.
    EscapeCommand,
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

    // RustPython specific.
    StartModule,
    StartInteractive,
    StartExpression,
}

impl TokenKind {
    #[inline]
    pub const fn is_newline(&self) -> bool {
        matches!(self, TokenKind::Newline | TokenKind::NonLogicalNewline)
    }

    #[inline]
    pub const fn is_unary(&self) -> bool {
        matches!(self, TokenKind::Plus | TokenKind::Minus)
    }

    #[inline]
    pub const fn is_keyword(&self) -> bool {
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
    pub const fn is_operator(&self) -> bool {
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
    pub const fn is_singleton(&self) -> bool {
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
    pub const fn is_arithmetic(&self) -> bool {
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
    pub const fn is_bitwise_or_shift(&self) -> bool {
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
    pub const fn is_soft_keyword(&self) -> bool {
        matches!(self, TokenKind::Match | TokenKind::Case)
    }

    pub const fn from_token(token: &Tok) -> Self {
        match token {
            Tok::Name { .. } => TokenKind::Name,
            Tok::Int { .. } => TokenKind::Int,
            Tok::Float { .. } => TokenKind::Float,
            Tok::Complex { .. } => TokenKind::Complex,
            Tok::String { .. } => TokenKind::String,
            Tok::IpyEscapeCommand { .. } => TokenKind::EscapeCommand,
            Tok::Comment(_) => TokenKind::Comment,
            Tok::Newline => TokenKind::Newline,
            Tok::NonLogicalNewline => TokenKind::NonLogicalNewline,
            Tok::Indent => TokenKind::Indent,
            Tok::Dedent => TokenKind::Dedent,
            Tok::EndOfFile => TokenKind::EndOfFile,
            Tok::Question => TokenKind::Question,
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
