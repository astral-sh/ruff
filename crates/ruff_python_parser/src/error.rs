use std::fmt;

use ruff_text_size::TextRange;

use crate::{
    lexer::{LexicalError, LexicalErrorType},
    Tok, TokenKind,
};

/// Represents represent errors that occur during parsing and are
/// returned by the `parse_*` functions.
#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub error: ParseErrorType,
    pub location: TextRange,
}

impl std::ops::Deref for ParseError {
    type Target = ParseErrorType;

    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} at byte range {:?}", &self.error, self.location)
    }
}

impl From<LexicalError> for ParseError {
    fn from(error: LexicalError) -> Self {
        ParseError {
            error: ParseErrorType::Lexical(error.error),
            location: error.location,
        }
    }
}

impl ParseError {
    pub fn error(self) -> ParseErrorType {
        self.error
    }
}

/// Represents the different types of errors that can occur during parsing of an f-string.
#[derive(Debug, Clone, PartialEq)]
pub enum FStringErrorType {
    /// Expected a right brace after an opened left brace.
    UnclosedLbrace,
    /// An invalid conversion flag was encountered.
    InvalidConversionFlag,
    /// A single right brace was encountered.
    SingleRbrace,
    /// Unterminated string.
    UnterminatedString,
    /// Unterminated triple-quoted string.
    UnterminatedTripleQuotedString,
    /// A lambda expression without parentheses was encountered.
    LambdaWithoutParentheses,
}

impl std::fmt::Display for FStringErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use FStringErrorType::{
            InvalidConversionFlag, LambdaWithoutParentheses, SingleRbrace, UnclosedLbrace,
            UnterminatedString, UnterminatedTripleQuotedString,
        };
        match self {
            UnclosedLbrace => write!(f, "expecting '}}'"),
            InvalidConversionFlag => write!(f, "invalid conversion character"),
            SingleRbrace => write!(f, "single '}}' is not allowed"),
            UnterminatedString => write!(f, "unterminated string"),
            UnterminatedTripleQuotedString => write!(f, "unterminated triple-quoted string"),
            LambdaWithoutParentheses => {
                write!(f, "lambda expressions are not allowed without parentheses")
            }
        }
    }
}

/// Represents the different types of errors that can occur during parsing.
#[derive(Debug, PartialEq)]
pub enum ParseErrorType {
    /// An unexpected error occurred.
    OtherError(String),
    /// An empty slice was found during parsing, e.g `l[]`.
    EmptySlice,
    /// An invalid expression was found in the assignment `target`.
    AssignmentError,
    /// An invalid expression was found in the named assignment `target`.
    NamedAssignmentError,
    /// An invalid expression was found in the augmented assignment `target`.
    AugAssignmentError,
    /// Multiple simple statements were found in the same line without a `;` separating them.
    SimpleStmtsInSameLine,
    /// An unexpected indentation was found during parsing.
    UnexpectedIndentation,
    /// The statement being parsed cannot be `async`.
    StmtIsNotAsync(TokenKind),
    /// A parameter was found after a vararg
    ParamFollowsVarKeywordParam,
    /// A positional argument follows a keyword argument.
    PositionalArgumentError,
    /// An iterable argument unpacking `*args` follows keyword argument unpacking `**kwargs`.
    UnpackedArgumentError,
    /// A non-default argument follows a default argument.
    DefaultArgumentError,
    /// A simple statement and a compound statement was found in the same line.
    SimpleStmtAndCompoundStmtInSameLine,
    /// An invalid `match` case pattern was found.
    InvalidMatchPatternLiteral { pattern: TokenKind },
    /// The parser expected a specific token that was not found.
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,
    },
    /// A duplicate argument was found in a function definition.
    DuplicateArgumentError(String),
    /// A keyword argument was repeated.
    DuplicateKeywordArgumentError(String),
    /// An f-string error containing the [`FStringErrorType`].
    FStringError(FStringErrorType),
    /// Parser encountered an error during lexing.
    Lexical(LexicalErrorType),

    // RustPython specific.
    /// Parser encountered an extra token
    ExtraToken(Tok),
    /// Parser encountered an invalid token
    InvalidToken,
    /// Parser encountered an unexpected token
    UnrecognizedToken(Tok, Option<String>),
    /// Parser encountered an unexpected end of input
    Eof,
}

impl std::error::Error for ParseErrorType {}

impl std::fmt::Display for ParseErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseErrorType::OtherError(msg) => write!(f, "{msg}"),
            ParseErrorType::ExpectedToken { found, expected } => {
                write!(f, "expected {expected:?}, found {found:?}")
            }
            ParseErrorType::Lexical(ref lex_error) => write!(f, "{lex_error}"),
            ParseErrorType::SimpleStmtsInSameLine => {
                write!(f, "use `;` to separate simple statements")
            }
            ParseErrorType::SimpleStmtAndCompoundStmtInSameLine => write!(
                f,
                "compound statements not allowed in the same line as simple statements"
            ),
            ParseErrorType::StmtIsNotAsync(kind) => {
                write!(f, "`{kind:?}` statement cannot be async")
            }
            ParseErrorType::UnpackedArgumentError => {
                write!(
                    f,
                    "iterable argument unpacking follows keyword argument unpacking"
                )
            }
            ParseErrorType::PositionalArgumentError => {
                write!(f, "positional argument follows keyword argument unpacking")
            }
            ParseErrorType::EmptySlice => write!(f, "slice cannot be empty"),
            ParseErrorType::ParamFollowsVarKeywordParam => {
                write!(f, "parameters cannot follow var-keyword parameter")
            }
            ParseErrorType::DefaultArgumentError => {
                write!(f, "non-default argument follows default argument")
            }
            ParseErrorType::InvalidMatchPatternLiteral { pattern } => {
                write!(f, "invalid pattern `{pattern:?}`")
            }
            ParseErrorType::UnexpectedIndentation => write!(f, "unexpected indentation"),
            ParseErrorType::AssignmentError => write!(f, "invalid assignment target"),
            ParseErrorType::NamedAssignmentError => write!(f, "invalid assignment target"),
            ParseErrorType::AugAssignmentError => write!(f, "invalid augmented assignment target"),
            ParseErrorType::DuplicateArgumentError(arg_name) => {
                write!(f, "duplicate argument '{arg_name}' in function definition")
            }
            ParseErrorType::DuplicateKeywordArgumentError(arg_name) => {
                write!(f, "keyword argument repeated: {arg_name}")
            }
            ParseErrorType::FStringError(ref fstring_error) => {
                write!(f, "f-string: {fstring_error}")
            }
            // RustPython specific.
            ParseErrorType::Eof => write!(f, "Got unexpected EOF"),
            ParseErrorType::ExtraToken(ref tok) => write!(f, "Got extraneous token: {tok:?}"),
            ParseErrorType::InvalidToken => write!(f, "Got invalid token"),
            ParseErrorType::UnrecognizedToken(ref tok, ref expected) => {
                if *tok == Tok::Indent {
                    write!(f, "unexpected indent")
                } else if expected.as_deref() == Some("Indent") {
                    write!(f, "expected an indented block")
                } else {
                    write!(f, "invalid syntax. Got unexpected token {tok}")
                }
            }
        }
    }
}
