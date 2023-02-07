//! Token type for Python source code created by the lexer and consumed by the parser.
//!
//! This module defines the tokens that the lexer recognizes. The tokens are
//! loosely based on the token definitions found in the [CPython source].
//!
//! [CPython source]: https://github.com/python/cpython/blob/dfc2e065a2e71011017077e549cd2f9bf4944c54/Include/internal/pycore_token.h
use num_bigint::BigInt;
use std::fmt;

/// The set of tokens the Python source code can be tokenized in.
#[derive(Clone, Debug, PartialEq)]
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
    With,
    Yield,

    // RustPython specific.
    StartModule,
    StartInteractive,
    StartExpression,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            Newline => f.write_str("Newline"),
            NonLogicalNewline => f.write_str("NonLogicalNewline"),
            Indent => f.write_str("Indent"),
            Dedent => f.write_str("Dedent"),
            StartModule => f.write_str("StartProgram"),
            StartInteractive => f.write_str("StartInteractive"),
            StartExpression => f.write_str("StartExpression"),
            EndOfFile => f.write_str("EOF"),
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
#[derive(PartialEq, Eq, Debug, Clone)]
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
        use StringKind::*;
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
    pub fn is_fstring(&self) -> bool {
        use StringKind::{FString, RawFString};
        matches!(self, FString | RawFString)
    }

    /// Returns true if the string is a byte string, i,e one of
    /// [`StringKind::Bytes`] or [`StringKind::RawBytes`].
    pub fn is_bytes(&self) -> bool {
        use StringKind::{Bytes, RawBytes};
        matches!(self, Bytes | RawBytes)
    }

    /// Returns true if the string is a unicode string, i,e [`StringKind::Unicode`].
    pub fn is_unicode(&self) -> bool {
        matches!(self, StringKind::Unicode)
    }

    /// Returns the number of characters in the prefix.
    pub fn prefix_len(&self) -> usize {
        use StringKind::*;
        match self {
            String => 0,
            RawString | FString | Unicode | Bytes => 1,
            RawFString | RawBytes => 2,
        }
    }
}
