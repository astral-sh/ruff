//! Define internal parse error types
//! The goal is to provide a matching and a safe error API, maksing errors from LALR

use crate::{ast::Location, token::Tok};
use lalrpop_util::ParseError as LalrpopError;
use std::{error::Error, fmt};

/// Represents an error during lexical scanning.
#[derive(Debug, PartialEq)]
pub struct LexicalError {
    pub error: LexicalErrorType,
    pub location: Location,
}

#[derive(Debug, PartialEq)]
pub enum LexicalErrorType {
    StringError,
    UnicodeError,
    NestingError,
    IndentationError,
    TabError,
    TabsAfterSpaces,
    DefaultArgumentError,
    PositionalArgumentError,
    DuplicateKeywordArgumentError,
    UnrecognizedToken { tok: char },
    FStringError(FStringErrorType),
    LineContinuationError,
    Eof,
    OtherError(String),
}

impl fmt::Display for LexicalErrorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LexicalErrorType::StringError => write!(f, "Got unexpected string"),
            LexicalErrorType::FStringError(error) => write!(f, "Got error in f-string: {}", error),
            LexicalErrorType::UnicodeError => write!(f, "Got unexpected unicode"),
            LexicalErrorType::NestingError => write!(f, "Got unexpected nesting"),
            LexicalErrorType::IndentationError => {
                write!(f, "unindent does not match any outer indentation level")
            }
            LexicalErrorType::TabError => {
                write!(f, "inconsistent use of tabs and spaces in indentation")
            }
            LexicalErrorType::TabsAfterSpaces => {
                write!(f, "Tabs not allowed as part of indentation after spaces")
            }
            LexicalErrorType::DefaultArgumentError => {
                write!(f, "non-default argument follows default argument")
            }
            LexicalErrorType::DuplicateKeywordArgumentError => {
                write!(f, "keyword argument repeated")
            }
            LexicalErrorType::PositionalArgumentError => {
                write!(f, "positional argument follows keyword argument")
            }
            LexicalErrorType::UnrecognizedToken { tok } => {
                write!(f, "Got unexpected token {}", tok)
            }
            LexicalErrorType::LineContinuationError => {
                write!(f, "unexpected character after line continuation character")
            }
            LexicalErrorType::Eof => write!(f, "unexpected EOF while parsing"),
            LexicalErrorType::OtherError(msg) => write!(f, "{}", msg),
        }
    }
}

// TODO: consolidate these with ParseError
#[derive(Debug, PartialEq)]
pub struct FStringError {
    pub error: FStringErrorType,
    pub location: Location,
}

#[derive(Debug, PartialEq)]
pub enum FStringErrorType {
    UnclosedLbrace,
    UnopenedRbrace,
    ExpectedRbrace,
    InvalidExpression(Box<ParseErrorType>),
    InvalidConversionFlag,
    EmptyExpression,
    MismatchedDelimiter,
    ExpressionNestedTooDeeply,
}

impl fmt::Display for FStringErrorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FStringErrorType::UnclosedLbrace => write!(f, "Unclosed '{{'"),
            FStringErrorType::UnopenedRbrace => write!(f, "Unopened '}}'"),
            FStringErrorType::ExpectedRbrace => write!(f, "Expected '}}' after conversion flag."),
            FStringErrorType::InvalidExpression(error) => {
                write!(f, "Invalid expression: {}", error)
            }
            FStringErrorType::InvalidConversionFlag => write!(f, "Invalid conversion flag"),
            FStringErrorType::EmptyExpression => write!(f, "Empty expression"),
            FStringErrorType::MismatchedDelimiter => write!(f, "Mismatched delimiter"),
            FStringErrorType::ExpressionNestedTooDeeply => {
                write!(f, "expressions nested too deeply")
            }
        }
    }
}

impl From<FStringError> for LalrpopError<Location, Tok, LexicalError> {
    fn from(err: FStringError) -> Self {
        lalrpop_util::ParseError::User {
            error: LexicalError {
                error: LexicalErrorType::FStringError(err.error),
                location: err.location,
            },
        }
    }
}

/// Represents an error during parsing
pub type ParseError = rustpython_compiler_core::BaseError<ParseErrorType>;

#[derive(Debug, PartialEq)]
pub enum ParseErrorType {
    /// Parser encountered an unexpected end of input
    Eof,
    /// Parser encountered an extra token
    ExtraToken(Tok),
    /// Parser encountered an invalid token
    InvalidToken,
    /// Parser encountered an unexpected token
    UnrecognizedToken(Tok, Option<String>),
    /// Maps to `User` type from `lalrpop-util`
    Lexical(LexicalErrorType),
}

/// Convert `lalrpop_util::ParseError` to our internal type
pub(crate) fn parse_error_from_lalrpop(
    err: LalrpopError<Location, Tok, LexicalError>,
    source_path: &str,
) -> ParseError {
    let source_path = source_path.to_owned();
    match err {
        // TODO: Are there cases where this isn't an EOF?
        LalrpopError::InvalidToken { location } => ParseError {
            error: ParseErrorType::Eof,
            location,
            source_path,
        },
        LalrpopError::ExtraToken { token } => ParseError {
            error: ParseErrorType::ExtraToken(token.1),
            location: token.0,
            source_path,
        },
        LalrpopError::User { error } => ParseError {
            error: ParseErrorType::Lexical(error.error),
            location: error.location,
            source_path,
        },
        LalrpopError::UnrecognizedToken { token, expected } => {
            // Hacky, but it's how CPython does it. See PyParser_AddToken,
            // in particular "Only one possible expected token" comment.
            let expected = (expected.len() == 1).then(|| expected[0].clone());
            ParseError {
                error: ParseErrorType::UnrecognizedToken(token.1, expected),
                location: token.0,
                source_path,
            }
        }
        LalrpopError::UnrecognizedEOF { location, .. } => ParseError {
            error: ParseErrorType::Eof,
            location,
            source_path,
        },
    }
}

impl fmt::Display for ParseErrorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ParseErrorType::Eof => write!(f, "Got unexpected EOF"),
            ParseErrorType::ExtraToken(ref tok) => write!(f, "Got extraneous token: {:?}", tok),
            ParseErrorType::InvalidToken => write!(f, "Got invalid token"),
            ParseErrorType::UnrecognizedToken(ref tok, ref expected) => {
                if *tok == Tok::Indent {
                    write!(f, "unexpected indent")
                } else if expected.as_deref() == Some("Indent") {
                    write!(f, "expected an indented block")
                } else {
                    write!(f, "invalid syntax. Got unexpected token {}", tok)
                }
            }
            ParseErrorType::Lexical(ref error) => write!(f, "{}", error),
        }
    }
}

impl Error for ParseErrorType {}

impl ParseErrorType {
    pub fn is_indentation_error(&self) -> bool {
        match self {
            ParseErrorType::Lexical(LexicalErrorType::IndentationError) => true,
            ParseErrorType::UnrecognizedToken(token, expected) => {
                *token == Tok::Indent || expected.clone() == Some("Indent".to_owned())
            }
            _ => false,
        }
    }
    pub fn is_tab_error(&self) -> bool {
        matches!(
            self,
            ParseErrorType::Lexical(LexicalErrorType::TabError)
                | ParseErrorType::Lexical(LexicalErrorType::TabsAfterSpaces)
        )
    }
}
