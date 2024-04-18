use std::fmt;

use ruff_text_size::TextRange;

use crate::lexer::{LexicalError, LexicalErrorType};
use crate::TokenKind;

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

    /// An empty slice was found during parsing, e.g `data[]`.
    EmptySlice,
    /// An empty global names list was found during parsing.
    EmptyGlobalNames,
    /// An empty nonlocal names list was found during parsing.
    EmptyNonlocalNames,
    /// An empty delete targets list was found during parsing.
    EmptyDeleteTargets,
    /// An empty import names list was found during parsing.
    EmptyImportNames,

    /// An unparenthesized named expression was found where it is not allowed.
    UnparenthesizedNamedExpression,
    /// An unparenthesized tuple expression was found where it is not allowed.
    UnparenthesizedTupleExpression,

    /// An invalid usage of a lambda expression was found.
    InvalidLambdaExpressionUsage,
    /// An invalid usage of a yield expression was found.
    InvalidYieldExpressionUsage,
    /// An invalid usage of a starred expression was found.
    InvalidStarredExpressionUsage,
    /// A star pattern was found outside a sequence pattern.
    InvalidStarPatternUsage,

    /// A parameter was found after a vararg.
    ParamAfterVarKeywordParam,
    /// A non-default parameter follows a default parameter.
    NonDefaultParamAfterDefaultParam,
    /// A default value was found for a `*` or `**` parameter.
    VarParameterWithDefault,

    /// A duplicate parameter was found in a function definition or lambda expression.
    DuplicateParameter(String),
    /// A keyword argument was repeated.
    DuplicateKeywordArgumentError(String),

    /// An invalid expression was found in the assignment target.
    InvalidAssignmentTarget,
    /// An invalid expression was found in the named assignment target.
    InvalidNamedAssignmentTarget,
    /// An invalid expression was found in the annotated assignment target.
    InvalidAnnotatedAssignmentTarget,
    /// An invalid expression was found in the augmented assignment target.
    InvalidAugmentedAssignmentTarget,
    /// An invalid expression was found in the delete target.
    InvalidDeleteTarget,

    /// A positional argument was found after a keyword argument.
    PositionalAfterKeywordArgument,
    /// A positional argument was found after a keyword argument unpacking.
    PositionalAfterKeywordUnpacking,
    /// An iterable argument unpacking was found after keyword argument unpacking.
    InvalidArgumentUnpackingOrder,
    /// An invalid usage of iterable unpacking in a comprehension was found.
    IterableUnpackingInComprehension,

    /// Multiple simple statements were found in the same line without a `;` separating them.
    SimpleStatementsOnSameLine,
    /// A simple statement and a compound statement was found in the same line.
    SimpleAndCompoundStatementOnSameLine,

    /// Expected one or more keyword parameter after `*` separator.
    ExpectedKeywordParam,
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

    /// An unexpected indentation was found during parsing.
    UnexpectedIndentation,
    /// The statement being parsed cannot be `async`.
    UnexpectedTokenAfterAsync(TokenKind),
    /// Ipython escape command was found
    UnexpectedIpythonEscapeCommand,
    /// An unexpected token was found at the end of an expression parsing
    UnexpectedExpressionToken,

    /// An f-string error containing the [`FStringErrorType`].
    FStringError(FStringErrorType),
    /// Parser encountered an error during lexing.
    Lexical(LexicalErrorType),
}

impl std::error::Error for ParseErrorType {}

impl std::fmt::Display for ParseErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseErrorType::OtherError(msg) => write!(f, "{msg}"),
            ParseErrorType::ExpectedToken { found, expected } => {
                write!(f, "Expected {expected}, found {found}",)
            }
            ParseErrorType::Lexical(ref lex_error) => write!(f, "{lex_error}"),
            ParseErrorType::SimpleStatementsOnSameLine => {
                f.write_str("Simple statements must be separated by newlines or semicolons")
            }
            ParseErrorType::SimpleAndCompoundStatementOnSameLine => f.write_str(
                "Compound statements are not allowed on the same line as simple statements",
            ),
            ParseErrorType::UnexpectedTokenAfterAsync(kind) => {
                write!(
                    f,
                    "Expected 'def', 'with' or 'for' to follow 'async', found {kind}",
                )
            }
            ParseErrorType::InvalidArgumentUnpackingOrder => {
                f.write_str("Iterable argument unpacking cannot follow keyword argument unpacking")
            }
            ParseErrorType::IterableUnpackingInComprehension => {
                f.write_str("Iterable unpacking cannot be used in a comprehension")
            }
            ParseErrorType::UnparenthesizedNamedExpression => {
                f.write_str("Unparenthesized named expression cannot be used here")
            }
            ParseErrorType::UnparenthesizedTupleExpression => {
                f.write_str("Unparenthesized tuple expression cannot be used here")
            }
            ParseErrorType::InvalidYieldExpressionUsage => {
                f.write_str("Yield expression cannot be used here")
            }
            ParseErrorType::InvalidLambdaExpressionUsage => {
                f.write_str("Lambda expression cannot be used here")
            }
            ParseErrorType::InvalidStarredExpressionUsage => {
                f.write_str("Starred expression cannot be used here")
            }
            ParseErrorType::PositionalAfterKeywordArgument => {
                f.write_str("Positional argument cannot follow keyword argument")
            }
            ParseErrorType::PositionalAfterKeywordUnpacking => {
                f.write_str("Positional argument cannot follow keyword argument unpacking")
            }
            ParseErrorType::EmptySlice => f.write_str("Expected index or slice expression"),
            ParseErrorType::EmptyGlobalNames => {
                f.write_str("Global statement must have at least one name")
            }
            ParseErrorType::EmptyNonlocalNames => {
                f.write_str("Nonlocal statement must have at least one name")
            }
            ParseErrorType::EmptyDeleteTargets => {
                f.write_str("Delete statement must have at least one target")
            }
            ParseErrorType::EmptyImportNames => {
                f.write_str("Expected one or more symbol names after import")
            }
            ParseErrorType::ParamAfterVarKeywordParam => {
                f.write_str("Parameter cannot follow var-keyword parameter")
            }
            ParseErrorType::NonDefaultParamAfterDefaultParam => {
                f.write_str("Parameter without a default cannot follow a parameter with a default")
            }
            ParseErrorType::ExpectedKeywordParam => {
                f.write_str("Expected one or more keyword parameter after '*' separator")
            }
            ParseErrorType::VarParameterWithDefault => {
                f.write_str("Parameter with '*' or '**' cannot have default value")
            }
            ParseErrorType::InvalidStarPatternUsage => {
                f.write_str("Star pattern cannot be used here")
            }
            ParseErrorType::ExpectedRealNumber => {
                f.write_str("Expected a real number in complex literal pattern")
            }
            ParseErrorType::ExpectedImaginaryNumber => {
                f.write_str("Expected an imaginary number in complex literal pattern")
            }
            ParseErrorType::ExpectedExpression => f.write_str("Expected an expression"),
            ParseErrorType::UnexpectedIndentation => f.write_str("Unexpected indentation"),
            ParseErrorType::InvalidAssignmentTarget => f.write_str("Invalid assignment target"),
            ParseErrorType::InvalidAnnotatedAssignmentTarget => {
                f.write_str("Invalid annotated assignment target")
            }
            ParseErrorType::InvalidNamedAssignmentTarget => {
                f.write_str("Assignment expression target must be an identifier")
            }
            ParseErrorType::InvalidAugmentedAssignmentTarget => {
                f.write_str("Invalid augmented assignment target")
            }
            ParseErrorType::InvalidDeleteTarget => f.write_str("Invalid delete target"),
            ParseErrorType::DuplicateParameter(arg_name) => {
                write!(f, "Duplicate parameter {arg_name:?}")
            }
            ParseErrorType::DuplicateKeywordArgumentError(arg_name) => {
                write!(f, "Duplicate keyword argument {arg_name:?}")
            }
            ParseErrorType::UnexpectedIpythonEscapeCommand => {
                f.write_str("IPython escape commands are only allowed in `Mode::Ipython`")
            }
            ParseErrorType::FStringError(ref fstring_error) => {
                write!(f, "f-string: {fstring_error}")
            }
            ParseErrorType::UnexpectedExpressionToken => {
                write!(f, "Unexpected token at the end of an expression")
            }
        }
    }
}
