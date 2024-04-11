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
            location: error.location(),
            error: ParseErrorType::Lexical(error.into_error()),
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
    /// An empty global names list was found during parsing, e.g `global`.
    EmptyGlobalNames,
    /// An empty nonlocal names list was found during parsing, e.g `nonlocal`.
    EmptyNonlocalNames,
    /// An empty delete targets list was found during parsing, e.g `del`.
    EmptyDeleteTargets,
    /// An empty import names list was found during parsing, e.g., `import`.
    EmptyImportNames,

    /// An unparenthesized named expression was found where it is not allowed.
    UnparenthesizedNamedExpression,
    /// An unparenthesized tuple expression was found where it is not allowed.
    UnparenthesizedTupleExpression,
    /// An invalid usage of a lambda expression was found.
    InvalidLambdaExpressionUsage,
    /// An invalid usage of a yield expression was found.
    InvalidYieldExpressionUsage,

    /// A parameter was found after a vararg.
    ParamFollowsVarKeywordParam,
    /// A non-default parameter follows a default parameter.
    NonDefaultParamFollowsDefaultParam,
    /// Expected one or more keyword parameter after `*` separator.
    ExpectedKeywordParam,
    /// A default value was found for a `*` or `**` parameter.
    VarParameterWithDefault,
    /// A duplicate parameter was found in a function definition or lambda expression.
    DuplicateParameter(String),

    /// An invalid expression was found in the assignment `target`.
    InvalidAssignmentTarget,
    /// An invalid expression was found in the named assignment `target`.
    InvalidNamedAssignmentTarget,
    /// An invalid expression was found in the annotated assignment `target`.
    InvalidAnnotatedAssignmentTarget,
    /// An invalid expression was found in the augmented assignment `target`.
    InvalidAugmentedAssignmentTarget,
    /// An invalid expression was found in the delete `target`.
    InvalidDeleteTarget,
    /// An unexpected indentation was found during parsing.
    UnexpectedIndentation,
    /// The statement being parsed cannot be `async`.
    UnexpectedAsyncToken(TokenKind),
    /// A positional argument follows a keyword argument.
    PositionalFollowsKeywordArgument,
    /// A positional argument follows a keyword argument unpacking.
    PositionalFollowsKeywordUnpacking,
    /// An iterable argument unpacking `*args` follows keyword argument unpacking `**kwargs`.
    UnpackedArgumentError,
    /// An invalid usage of iterable unpacking in a comprehension was found.
    IterableUnpackingInComprehension,
    /// An invalid usage of a starred expression was found.
    StarredExpressionUsage,

    /// Multiple simple statements were found in the same line without a `;` separating them.
    SimpleStatementsOnSameLine,
    /// A simple statement and a compound statement was found in the same line.
    SimpleAndCompoundStatementOnSameLine,

    /// An invalid `match` case pattern was found.
    InvalidMatchPatternLiteral { pattern: TokenKind },
    /// A star pattern was found outside a sequence pattern.
    StarPatternUsageError,
    /// Expected a real number for a complex literal pattern.
    ExpectedRealNumber,
    /// Expected an imaginary number for a complex literal pattern.
    ExpectedImaginaryNumber,
    /// Expected an expression at the current parser location.
    ExpectedExpression,

    /// The parser expected a specific token that was not found.
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,
    },
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
            ParseErrorType::SimpleStatementsOnSameLine => {
                write!(
                    f,
                    "Simple statements must be separated by newlines or semicolons"
                )
            }
            ParseErrorType::SimpleAndCompoundStatementOnSameLine => write!(
                f,
                "Compound statements are not allowed on the same line as simple statements"
            ),
            ParseErrorType::UnexpectedAsyncToken(kind) => {
                write!(
                    f,
                    "Expected 'def', 'with' or 'for' to follow 'async', found {kind:?}"
                )
            }
            ParseErrorType::UnpackedArgumentError => {
                write!(
                    f,
                    "iterable argument unpacking follows keyword argument unpacking"
                )
            }
            ParseErrorType::IterableUnpackingInComprehension => {
                write!(f, "iterable unpacking cannot be used in a comprehension")
            }
            ParseErrorType::UnparenthesizedNamedExpression => {
                write!(f, "unparenthesized named expression cannot be used here")
            }
            ParseErrorType::UnparenthesizedTupleExpression => {
                write!(f, "unparenthesized tuple expression cannot be used here")
            }
            ParseErrorType::InvalidYieldExpressionUsage => {
                write!(f, "yield expression cannot be used here")
            }
            ParseErrorType::InvalidLambdaExpressionUsage => {
                write!(f, "`lambda` expression cannot be used here")
            }
            ParseErrorType::StarredExpressionUsage => {
                write!(f, "starred expression cannot be used here")
            }
            ParseErrorType::PositionalFollowsKeywordArgument => {
                write!(f, "positional argument follows keyword argument")
            }
            ParseErrorType::PositionalFollowsKeywordUnpacking => {
                write!(f, "positional argument follows keyword argument unpacking")
            }
            ParseErrorType::EmptySlice => write!(f, "slice cannot be empty"),
            ParseErrorType::EmptyGlobalNames => {
                f.write_str("`global` statement must have at least one name")
            }
            ParseErrorType::EmptyNonlocalNames => {
                f.write_str("`nonlocal` statement must have at least one name")
            }
            ParseErrorType::EmptyDeleteTargets => {
                f.write_str("`del` statement must have at least one target")
            }
            ParseErrorType::EmptyImportNames => {
                f.write_str("Expected one or more symbol names after import")
            }
            ParseErrorType::ParamFollowsVarKeywordParam => {
                write!(f, "Parameter cannot follow var-keyword parameter")
            }
            ParseErrorType::NonDefaultParamFollowsDefaultParam => {
                write!(
                    f,
                    "Parameter without a default cannot follow a parameter with a default"
                )
            }
            ParseErrorType::ExpectedKeywordParam => {
                write!(
                    f,
                    "Expected one or more keyword parameter after '*' separator"
                )
            }
            ParseErrorType::VarParameterWithDefault => {
                write!(f, "Parameter with '*' or '**' cannot have default value")
            }
            ParseErrorType::InvalidMatchPatternLiteral { pattern } => {
                write!(f, "invalid pattern `{pattern:?}`")
            }
            ParseErrorType::StarPatternUsageError => {
                write!(f, "Star pattern cannot be used here")
            }
            ParseErrorType::ExpectedRealNumber => {
                write!(f, "Expected a real number in complex literal pattern")
            }
            ParseErrorType::ExpectedImaginaryNumber => {
                write!(f, "Expected an imaginary number in complex literal pattern")
            }
            ParseErrorType::ExpectedExpression => write!(f, "Expected an expression"),
            ParseErrorType::UnexpectedIndentation => write!(f, "unexpected indentation"),
            ParseErrorType::InvalidAssignmentTarget => write!(f, "invalid assignment target"),
            ParseErrorType::InvalidAnnotatedAssignmentTarget => {
                write!(f, "invalid annotated assignment target")
            }
            ParseErrorType::InvalidNamedAssignmentTarget => {
                write!(f, "assignment expression target must be an identifier")
            }
            ParseErrorType::InvalidAugmentedAssignmentTarget => {
                write!(f, "invalid augmented assignment target")
            }
            ParseErrorType::InvalidDeleteTarget => {
                write!(f, "invalid delete target")
            }
            ParseErrorType::DuplicateParameter(arg_name) => {
                write!(f, "Duplicate parameter '{arg_name}'")
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
                    write!(f, "Unexpected indent")
                } else if expected.as_deref() == Some("Indent") {
                    write!(f, "Expected an indented block")
                } else {
                    write!(f, "Unexpected token {tok}")
                }
            }
        }
    }
}
