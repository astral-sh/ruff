use rustpython_parser::Tok;

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
    Case,
    With,
    Yield,

    // RustPython specific.
    StartModule,
    StartInteractive,
    StartExpression,
}

#[inline]
pub const fn is_unary_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Plus | Tok::Minus | Tok::Star | Tok::DoubleStar | Tok::RightShift
    )
}

#[inline]
pub const fn is_keyword_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::False
            | Tok::True
            | Tok::None
            | Tok::And
            | Tok::As
            | Tok::Assert
            | Tok::Await
            | Tok::Break
            | Tok::Class
            | Tok::Continue
            | Tok::Def
            | Tok::Del
            | Tok::Elif
            | Tok::Else
            | Tok::Except
            | Tok::Finally
            | Tok::For
            | Tok::From
            | Tok::Global
            | Tok::If
            | Tok::Import
            | Tok::In
            | Tok::Is
            | Tok::Lambda
            | Tok::Nonlocal
            | Tok::Not
            | Tok::Or
            | Tok::Pass
            | Tok::Raise
            | Tok::Return
            | Tok::Try
            | Tok::While
            | Tok::With
            | Tok::Yield
    )
}

#[inline]
pub const fn is_operator_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Lpar
            | Tok::Rpar
            | Tok::Lsqb
            | Tok::Rsqb
            | Tok::Comma
            | Tok::Semi
            | Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::Vbar
            | Tok::Amper
            | Tok::Less
            | Tok::Greater
            | Tok::Equal
            | Tok::Dot
            | Tok::Percent
            | Tok::Lbrace
            | Tok::Rbrace
            | Tok::NotEqual
            | Tok::LessEqual
            | Tok::GreaterEqual
            | Tok::Tilde
            | Tok::CircumFlex
            | Tok::LeftShift
            | Tok::RightShift
            | Tok::DoubleStar
            | Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::PercentEqual
            | Tok::AmperEqual
            | Tok::VbarEqual
            | Tok::CircumflexEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::DoubleStarEqual
            | Tok::DoubleSlash
            | Tok::DoubleSlashEqual
            | Tok::At
            | Tok::AtEqual
            | Tok::Rarrow
            | Tok::Ellipsis
            | Tok::ColonEqual
            | Tok::Colon
    )
}

#[inline]
pub const fn is_singleton_token(token: &Tok) -> bool {
    matches!(token, Tok::False | Tok::True | Tok::None)
}

#[inline]
pub const fn is_arithmetic_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::DoubleStar | Tok::Star | Tok::Plus | Tok::Minus | Tok::Slash | Tok::At
    )
}

#[inline]
pub const fn is_soft_keyword_token(token: &Tok) -> bool {
    matches!(token, Tok::Match | Tok::Case)
}

impl TokenKind {
    pub const fn from_token(token: &Tok) -> Self {
        match token {
            Tok::Name { .. } => TokenKind::Name,
            Tok::Int { .. } => TokenKind::Int,
            Tok::Float { .. } => TokenKind::Float,
            Tok::Complex { .. } => TokenKind::Complex,
            Tok::String { .. } => TokenKind::String,
            Tok::Comment(_) => TokenKind::Comment,
            Tok::Newline => TokenKind::Newline,
            Tok::NonLogicalNewline => TokenKind::NonLogicalNewline,
            Tok::Indent => TokenKind::Indent,
            Tok::Dedent => TokenKind::Dedent,
            Tok::EndOfFile => TokenKind::EndOfFile,
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
            Tok::With => TokenKind::With,
            Tok::Yield => TokenKind::Yield,
            Tok::StartModule => TokenKind::StartModule,
            Tok::StartInteractive => TokenKind::StartInteractive,
            Tok::StartExpression => TokenKind::StartExpression,
        }
    }
}

impl From<&Tok> for TokenKind {
    fn from(value: &Tok) -> Self {
        Self::from_token(value)
    }
}
