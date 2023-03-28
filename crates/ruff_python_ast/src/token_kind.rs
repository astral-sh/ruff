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

impl TokenKind {
    pub const fn is_whitespace_needed(&self) -> bool {
        matches!(
            self,
            TokenKind::DoubleStarEqual
                | TokenKind::StarEqual
                | TokenKind::SlashEqual
                | TokenKind::DoubleSlashEqual
                | TokenKind::PlusEqual
                | TokenKind::MinusEqual
                | TokenKind::NotEqual
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::PercentEqual
                | TokenKind::CircumflexEqual
                | TokenKind::AmperEqual
                | TokenKind::VbarEqual
                | TokenKind::EqEqual
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::LeftShiftEqual
                | TokenKind::RightShiftEqual
                | TokenKind::Equal
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::In
                | TokenKind::Is
                | TokenKind::Rarrow
        )
    }

    pub const fn is_whitespace_optional(&self) -> bool {
        self.is_arithmetic()
            || matches!(
                self,
                TokenKind::CircumFlex
                    | TokenKind::Amper
                    | TokenKind::Vbar
                    | TokenKind::LeftShift
                    | TokenKind::RightShift
                    | TokenKind::Percent
            )
    }

    pub const fn is_unary(&self) -> bool {
        matches!(
            self,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::DoubleStar
                | TokenKind::RightShift
        )
    }

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
        )
    }

    pub const fn is_singleton(&self) -> bool {
        matches!(self, TokenKind::False | TokenKind::True | TokenKind::None)
    }

    pub const fn is_skip_comment(&self) -> bool {
        matches!(
            self,
            TokenKind::Newline
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::NonLogicalNewline
                | TokenKind::Comment
        )
    }

    pub const fn is_arithmetic(&self) -> bool {
        matches!(
            self,
            TokenKind::DoubleStar
                | TokenKind::Star
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Slash
                | TokenKind::At
        )
    }

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
