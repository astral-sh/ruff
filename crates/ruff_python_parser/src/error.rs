use std::fmt::{self, Display};

use ruff_python_ast::PythonVersion;
use ruff_text_size::{Ranged, TextRange};

use crate::TokenKind;

/// Represents represent errors that occur during parsing and are
/// returned by the `parse_*` functions.
#[derive(Debug, PartialEq, Clone)]
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
#[derive(Debug, PartialEq, Clone)]
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
    /// An empty type parameter list was found during parsing.
    EmptyTypeParams,

    /// An unparenthesized named expression was found where it is not allowed.
    UnparenthesizedNamedExpression,
    /// An unparenthesized tuple expression was found where it is not allowed.
    UnparenthesizedTupleExpression,
    /// An unparenthesized generator expression was found where it is not allowed.
    UnparenthesizedGeneratorExpression,

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
            ParseErrorType::UnparenthesizedGeneratorExpression => {
                f.write_str("Unparenthesized generator expression cannot be used here")
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
            ParseErrorType::EmptyTypeParams => f.write_str("Type parameter list cannot be empty"),
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

/// Represents an error that occur during lexing and are
/// returned by the `parse_*` functions in the iterator in the
/// [lexer] implementation.
///
/// [lexer]: crate::lexer
#[derive(Debug, Clone, PartialEq)]
pub struct LexicalError {
    /// The type of error that occurred.
    error: LexicalErrorType,
    /// The location of the error.
    location: TextRange,
}

impl LexicalError {
    /// Creates a new `LexicalError` with the given error type and location.
    pub fn new(error: LexicalErrorType, location: TextRange) -> Self {
        Self { error, location }
    }

    pub fn error(&self) -> &LexicalErrorType {
        &self.error
    }

    pub fn into_error(self) -> LexicalErrorType {
        self.error
    }

    pub fn location(&self) -> TextRange {
        self.location
    }
}

impl std::ops::Deref for LexicalError {
    type Target = LexicalErrorType;

    fn deref(&self) -> &Self::Target {
        self.error()
    }
}

impl std::error::Error for LexicalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error())
    }
}

impl std::fmt::Display for LexicalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} at byte offset {}",
            self.error(),
            u32::from(self.location().start())
        )
    }
}

/// Represents the different types of errors that can occur during lexing.
#[derive(Debug, Clone, PartialEq)]
pub enum LexicalErrorType {
    // TODO: Can probably be removed, the places it is used seem to be able
    // to use the `UnicodeError` variant instead.
    #[doc(hidden)]
    StringError,
    /// A string literal without the closing quote.
    UnclosedStringError,
    /// Decoding of a unicode escape sequence in a string literal failed.
    UnicodeError,
    /// Missing the `{` for unicode escape sequence.
    MissingUnicodeLbrace,
    /// Missing the `}` for unicode escape sequence.
    MissingUnicodeRbrace,
    /// The indentation is not consistent.
    IndentationError,
    /// An unrecognized token was encountered.
    UnrecognizedToken { tok: char },
    /// An f-string error containing the [`FStringErrorType`].
    FStringError(FStringErrorType),
    /// Invalid character encountered in a byte literal.
    InvalidByteLiteral,
    /// An unexpected character was encountered after a line continuation.
    LineContinuationError,
    /// An unexpected end of file was encountered.
    Eof,
    /// An unexpected error occurred.
    OtherError(Box<str>),
}

impl std::error::Error for LexicalErrorType {}

impl std::fmt::Display for LexicalErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LexicalErrorType::StringError => write!(f, "Got unexpected string"),
            LexicalErrorType::FStringError(error) => write!(f, "f-string: {error}"),
            LexicalErrorType::InvalidByteLiteral => {
                write!(f, "bytes can only contain ASCII literal characters")
            }
            LexicalErrorType::UnicodeError => write!(f, "Got unexpected unicode"),
            LexicalErrorType::IndentationError => {
                write!(f, "unindent does not match any outer indentation level")
            }
            LexicalErrorType::UnrecognizedToken { tok } => {
                write!(f, "Got unexpected token {tok}")
            }
            LexicalErrorType::LineContinuationError => {
                write!(f, "Expected a newline after line continuation character")
            }
            LexicalErrorType::Eof => write!(f, "unexpected EOF while parsing"),
            LexicalErrorType::OtherError(msg) => write!(f, "{msg}"),
            LexicalErrorType::UnclosedStringError => {
                write!(f, "missing closing quote in string literal")
            }
            LexicalErrorType::MissingUnicodeLbrace => {
                write!(f, "Missing `{{` in Unicode escape sequence")
            }
            LexicalErrorType::MissingUnicodeRbrace => {
                write!(f, "Missing `}}` in Unicode escape sequence")
            }
        }
    }
}

/// Represents a version-related syntax error detected during parsing.
///
/// An example of a version-related error is the use of a `match` statement before Python 3.10, when
/// it was first introduced. See [`UnsupportedSyntaxErrorKind`] for other kinds of errors.
#[derive(Debug, PartialEq, Clone)]
pub struct UnsupportedSyntaxError {
    pub kind: UnsupportedSyntaxErrorKind,
    pub range: TextRange,
    /// The target [`PythonVersion`] for which this error was detected.
    pub target_version: PythonVersion,
}

impl Ranged for UnsupportedSyntaxError {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// The type of tuple unpacking for [`UnsupportedSyntaxErrorKind::StarTuple`].
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum StarTupleKind {
    Return,
    Yield,
}

/// The type of PEP 701 f-string error for [`UnsupportedSyntaxErrorKind::Pep701FString`].
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum FStringKind {
    Backslash,
    Comment,
    NestedQuote,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum UnparenthesizedNamedExprKind {
    SequenceIndex,
    SetLiteral,
    SetComprehension,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum UnsupportedSyntaxErrorKind {
    Match,
    Walrus,
    ExceptStar,
    /// Represents the use of an unparenthesized named expression (`:=`) in a set literal, set
    /// comprehension, or sequence index before Python 3.10.
    ///
    /// ## Examples
    ///
    /// These are allowed on Python 3.10:
    ///
    /// ```python
    /// {x := 1, 2, 3}                 # set literal
    /// {last := x for x in range(3)}  # set comprehension
    /// lst[x := 1]                    # sequence index
    /// ```
    ///
    /// But on Python 3.9 the named expression needs to be parenthesized:
    ///
    /// ```python
    /// {(x := 1), 2, 3}                 # set literal
    /// {(last := x) for x in range(3)}  # set comprehension
    /// lst[(x := 1)]                    # sequence index
    /// ```
    ///
    /// However, unparenthesized named expressions are never allowed in slices:
    ///
    /// ```python
    /// lst[x:=1:-1]      # syntax error
    /// lst[1:x:=1]       # syntax error
    /// lst[1:3:x:=1]     # syntax error
    ///
    /// lst[(x:=1):-1]    # ok
    /// lst[1:(x:=1)]     # ok
    /// lst[1:3:(x:=1)]   # ok
    /// ```
    ///
    /// ## References
    ///
    /// - [Python 3.10 Other Language Changes](https://docs.python.org/3/whatsnew/3.10.html#other-language-changes)
    UnparenthesizedNamedExpr(UnparenthesizedNamedExprKind),

    /// Represents the use of a parenthesized keyword argument name after Python 3.8.
    ///
    /// ## Example
    ///
    /// From [BPO 34641] it sounds like this was only accidentally supported and was removed when
    /// noticed. Code like this used to be valid:
    ///
    /// ```python
    /// f((a)=1)
    /// ```
    ///
    /// After Python 3.8, you have to omit the parentheses around `a`:
    ///
    /// ```python
    /// f(a=1)
    /// ```
    ///
    /// [BPO 34641]: https://github.com/python/cpython/issues/78822
    ParenthesizedKeywordArgumentName,

    /// Represents the use of unparenthesized tuple unpacking in a `return` statement or `yield`
    /// expression before Python 3.8.
    ///
    /// ## Examples
    ///
    /// Before Python 3.8, this syntax was allowed:
    ///
    /// ```python
    /// rest = (4, 5, 6)
    ///
    /// def f():
    ///     t = 1, 2, 3, *rest
    ///     return t
    ///
    /// def g():
    ///     t = 1, 2, 3, *rest
    ///     yield t
    /// ```
    ///
    /// But this was not:
    ///
    /// ```python
    /// rest = (4, 5, 6)
    ///
    /// def f():
    ///     return 1, 2, 3, *rest
    ///
    /// def g():
    ///     yield 1, 2, 3, *rest
    /// ```
    ///
    /// Instead, parentheses were required in the `return` and `yield` cases:
    ///
    /// ```python
    /// rest = (4, 5, 6)
    ///
    /// def f():
    ///     return (1, 2, 3, *rest)
    ///
    /// def g():
    ///     yield (1, 2, 3, *rest)
    /// ```
    ///
    /// This was reported in [BPO 32117] and updated in Python 3.8 to allow the unparenthesized
    /// form.
    ///
    /// [BPO 32117]: https://github.com/python/cpython/issues/76298
    StarTuple(StarTupleKind),

    /// Represents the use of a "relaxed" [PEP 614] decorator before Python 3.9.
    ///
    /// ## Examples
    ///
    /// Prior to Python 3.9, decorators were defined to be [`dotted_name`]s, optionally followed by
    /// an argument list. For example:
    ///
    /// ```python
    /// @buttons.clicked.connect
    /// def foo(): ...
    ///
    /// @buttons.clicked.connect(1, 2, 3)
    /// def foo(): ...
    /// ```
    ///
    /// As pointed out in the PEP, this prevented reasonable extensions like subscripts:
    ///
    /// ```python
    /// buttons = [QPushButton(f'Button {i}') for i in range(10)]
    ///
    /// @buttons[0].clicked.connect
    /// def spam(): ...
    /// ```
    ///
    /// Python 3.9 removed these restrictions and expanded the [decorator grammar] to include any
    /// assignment expression and include cases like the example above.
    ///
    /// [PEP 614]: https://peps.python.org/pep-0614/
    /// [`dotted_name`]: https://docs.python.org/3.8/reference/compound_stmts.html#grammar-token-dotted-name
    /// [decorator grammar]: https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-decorator
    RelaxedDecorator(RelaxedDecoratorError),

    /// Represents the use of a [PEP 570] positional-only parameter before Python 3.8.
    ///
    /// ## Examples
    ///
    /// Python 3.8 added the `/` syntax for marking preceding parameters as positional-only:
    ///
    /// ```python
    /// def foo(a, b, /, c): ...
    /// ```
    ///
    /// This means `a` and `b` in this case can only be provided by position, not by name. In other
    /// words, this code results in a `TypeError` at runtime:
    ///
    /// ```pycon
    /// >>> def foo(a, b, /, c): ...
    /// ...
    /// >>> foo(a=1, b=2, c=3)
    /// Traceback (most recent call last):
    ///   File "<python-input-3>", line 1, in <module>
    ///     foo(a=1, b=2, c=3)
    ///     ~~~^^^^^^^^^^^^^^^
    /// TypeError: foo() got some positional-only arguments passed as keyword arguments: 'a, b'
    /// ```
    ///
    /// [PEP 570]: https://peps.python.org/pep-0570/
    PositionalOnlyParameter,

    /// Represents the use of a [type parameter list] before Python 3.12.
    ///
    /// ## Examples
    ///
    /// Before Python 3.12, generic parameters had to be declared separately using a class like
    /// [`typing.TypeVar`], which could then be used in a function or class definition:
    ///
    /// ```python
    /// from typing import Generic, TypeVar
    ///
    /// T = TypeVar("T")
    ///
    /// def f(t: T): ...
    /// class C(Generic[T]): ...
    /// ```
    ///
    /// [PEP 695], included in Python 3.12, introduced the new type parameter syntax, which allows
    /// these to be written more compactly and without a separate type variable:
    ///
    /// ```python
    /// def f[T](t: T): ...
    /// class C[T]: ...
    /// ```
    ///
    /// [type parameter list]: https://docs.python.org/3/reference/compound_stmts.html#type-parameter-lists
    /// [PEP 695]: https://peps.python.org/pep-0695/
    /// [`typing.TypeVar`]: https://docs.python.org/3/library/typing.html#typevar
    TypeParameterList,
    TypeAliasStatement,
    TypeParamDefault,

    /// Represents the use of a [PEP 701] f-string before Python 3.12.
    ///
    /// ## Examples
    ///
    /// As described in the PEP, each of these cases were invalid before Python 3.12:
    ///
    /// ```python
    /// # nested quotes
    /// f'Magic wand: { bag['wand'] }'
    ///
    /// # escape characters
    /// f"{'\n'.join(a)}"
    ///
    /// # comments
    /// f'''A complex trick: {
    ///     bag['bag']  # recursive bags!
    /// }'''
    ///
    /// # arbitrary nesting
    /// f"{f"{f"{f"{f"{f"{1+1}"}"}"}"}"}"
    /// ```
    ///
    /// These restrictions were lifted in Python 3.12, meaning that all of these examples are now
    /// valid.
    ///
    /// [PEP 701]: https://peps.python.org/pep-0701/
    Pep701FString(FStringKind),

    /// Represents the use of a parenthesized `with` item before Python 3.9.
    ///
    /// ## Examples
    ///
    /// As described in [BPO 12782], `with` uses like this were not allowed on Python 3.8:
    ///
    /// ```python
    /// with (open("a_really_long_foo") as foo,
    ///       open("a_really_long_bar") as bar):
    ///     pass
    /// ```
    ///
    /// because parentheses were not allowed within the `with` statement itself (see [this comment]
    /// in particular). However, parenthesized expressions were still allowed, including the cases
    /// below, so the issue can be pretty subtle and relates specifically to parenthesized items
    /// with `as` bindings.
    ///
    /// ```python
    /// with (foo, bar): ...  # okay
    /// with (
    ///   open('foo.txt')) as foo: ...  # also okay
    /// with (
    ///   foo,
    ///   bar,
    ///   baz,
    /// ): ...  # also okay, just a tuple
    /// with (
    ///   foo,
    ///   bar,
    ///   baz,
    /// ) as tup: ...  # also okay, binding the tuple
    /// ```
    ///
    /// This restriction was lifted in 3.9 but formally included in the [release notes] for 3.10.
    ///
    /// [BPO 12782]: https://github.com/python/cpython/issues/56991
    /// [this comment]: https://github.com/python/cpython/issues/56991#issuecomment-1093555141
    /// [release notes]: https://docs.python.org/3/whatsnew/3.10.html#summary-release-highlights
    ParenthesizedContextManager,

    /// Represents the use of a [PEP 646] star expression in an index.
    ///
    /// ## Examples
    ///
    /// Before Python 3.11, star expressions were not allowed in index/subscript operations (within
    /// square brackets). This restriction was lifted in [PEP 646] to allow for star-unpacking of
    /// `typing.TypeVarTuple`s, also added in Python 3.11. As such, this is the primary motivating
    /// example from the PEP:
    ///
    /// ```python
    /// from typing import TypeVar, TypeVarTuple
    ///
    /// DType = TypeVar('DType')
    /// Shape = TypeVarTuple('Shape')
    ///
    /// class Array(Generic[DType, *Shape]): ...
    /// ```
    ///
    /// But it applies to simple indexing as well:
    ///
    /// ```python
    /// vector[*x]
    /// array[a, *b]
    /// ```
    ///
    /// [PEP 646]: https://peps.python.org/pep-0646/#change-1-star-expressions-in-indexes
    StarExpressionInIndex,

    /// Represents the use of a [PEP 646] star annotations in a function definition.
    ///
    /// ## Examples
    ///
    /// Before Python 3.11, star annotations were not allowed in function definitions. This
    /// restriction was lifted in [PEP 646] to allow type annotations for `typing.TypeVarTuple`,
    /// also added in Python 3.11:
    ///
    /// ```python
    /// from typing import TypeVarTuple
    ///
    /// Ts = TypeVarTuple('Ts')
    ///
    /// def foo(*args: *Ts): ...
    /// ```
    ///
    /// Unlike [`UnsupportedSyntaxErrorKind::StarExpressionInIndex`], this does not include any
    /// other annotation positions:
    ///
    /// ```python
    /// x: *Ts                # Syntax error
    /// def foo(x: *Ts): ...  # Syntax error
    /// ```
    ///
    /// [PEP 646]: https://peps.python.org/pep-0646/#change-2-args-as-a-typevartuple
    StarAnnotation,

    /// Represents the use of tuple unpacking in a `for` statement iterator clause before Python
    /// 3.9.
    ///
    /// ## Examples
    ///
    /// Like [`UnsupportedSyntaxErrorKind::StarTuple`] in `return` and `yield` statements, prior to
    /// Python 3.9, tuple unpacking in the iterator clause of a `for` statement required
    /// parentheses:
    ///
    /// ```python
    /// # valid on Python 3.8 and earlier
    /// for i in (*a, *b): ...
    /// ```
    ///
    /// Omitting the parentheses was invalid:
    ///
    /// ```python
    /// for i in *a, *b: ...  # SyntaxError
    /// ```
    ///
    /// This was changed as part of the [PEG parser rewrite] included in Python 3.9 but not
    /// documented directly until the [Python 3.11 release].
    ///
    /// [PEG parser rewrite]: https://peps.python.org/pep-0617/
    /// [Python 3.11 release]: https://docs.python.org/3/whatsnew/3.11.html#other-language-changes
    UnparenthesizedUnpackInFor,
}

impl Display for UnsupportedSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            UnsupportedSyntaxErrorKind::Match => "Cannot use `match` statement",
            UnsupportedSyntaxErrorKind::Walrus => "Cannot use named assignment expression (`:=`)",
            UnsupportedSyntaxErrorKind::ExceptStar => "Cannot use `except*`",
            UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                UnparenthesizedNamedExprKind::SequenceIndex,
            ) => "Cannot use unparenthesized assignment expression in a sequence index",
            UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                UnparenthesizedNamedExprKind::SetLiteral,
            ) => "Cannot use unparenthesized assignment expression as an element in a set literal",
            UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(
                UnparenthesizedNamedExprKind::SetComprehension,
            ) => "Cannot use unparenthesized assignment expression as an element in a set comprehension",
            UnsupportedSyntaxErrorKind::ParenthesizedKeywordArgumentName => {
                "Cannot use parenthesized keyword argument name"
            }
            UnsupportedSyntaxErrorKind::StarTuple(StarTupleKind::Return) => {
                "Cannot use iterable unpacking in return statements"
            }
            UnsupportedSyntaxErrorKind::StarTuple(StarTupleKind::Yield) => {
                "Cannot use iterable unpacking in yield expressions"
            }
            UnsupportedSyntaxErrorKind::RelaxedDecorator(relaxed_decorator_error) => {
                return match relaxed_decorator_error {
                    RelaxedDecoratorError::CallExpression => {
                        write!(
                            f,
                            "Cannot use a call expression in a decorator on Python {} \
                            unless it is the top-level expression or it occurs \
                            in the argument list of a top-level call expression \
                            (relaxed decorator syntax was {changed})",
                            self.target_version,
                            changed = self.kind.changed_version(),
                        )
                    }
                    RelaxedDecoratorError::Other(description) => write!(
                        f,
                        "Cannot use {description} outside function call arguments in a decorator on Python {} \
                        (syntax was {changed})",
                        self.target_version,
                        changed = self.kind.changed_version(),
                    ),
                }
            }
            UnsupportedSyntaxErrorKind::PositionalOnlyParameter => {
                "Cannot use positional-only parameter separator"
            }
            UnsupportedSyntaxErrorKind::TypeParameterList => "Cannot use type parameter lists",
            UnsupportedSyntaxErrorKind::TypeAliasStatement => "Cannot use `type` alias statement",
            UnsupportedSyntaxErrorKind::TypeParamDefault => {
                "Cannot set default type for a type parameter"
            }
            UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Backslash) => {
                "Cannot use an escape sequence (backslash) in f-strings"
            }
            UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::Comment) => {
                "Cannot use comments in f-strings"
            }
            UnsupportedSyntaxErrorKind::Pep701FString(FStringKind::NestedQuote) => {
                "Cannot reuse outer quote character in f-strings"
            }
            UnsupportedSyntaxErrorKind::ParenthesizedContextManager => {
                "Cannot use parentheses within a `with` statement"
            }
            UnsupportedSyntaxErrorKind::StarExpressionInIndex => {
                "Cannot use star expression in index"
            }
            UnsupportedSyntaxErrorKind::StarAnnotation => "Cannot use star annotation",
            UnsupportedSyntaxErrorKind::UnparenthesizedUnpackInFor => {
                "Cannot use iterable unpacking in `for` statements"
            }
        };

        write!(
            f,
            "{kind} on Python {} (syntax was {changed})",
            self.target_version,
            changed = self.kind.changed_version(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelaxedDecoratorError {
    CallExpression,
    Other(&'static str),
}

/// Represents the kind of change in Python syntax between versions.
enum Change {
    Added(PythonVersion),
    Removed(PythonVersion),
}

impl Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Change::Added(version) => write!(f, "added in Python {version}"),
            Change::Removed(version) => write!(f, "removed in Python {version}"),
        }
    }
}

impl UnsupportedSyntaxErrorKind {
    /// Returns the Python version when the syntax associated with this error was changed, and the
    /// type of [`Change`] (added or removed).
    const fn changed_version(self) -> Change {
        match self {
            UnsupportedSyntaxErrorKind::Match => Change::Added(PythonVersion::PY310),
            UnsupportedSyntaxErrorKind::Walrus => Change::Added(PythonVersion::PY38),
            UnsupportedSyntaxErrorKind::ExceptStar => Change::Added(PythonVersion::PY311),
            UnsupportedSyntaxErrorKind::UnparenthesizedNamedExpr(_) => {
                Change::Added(PythonVersion::PY39)
            }
            UnsupportedSyntaxErrorKind::StarTuple(_) => Change::Added(PythonVersion::PY38),
            UnsupportedSyntaxErrorKind::RelaxedDecorator { .. } => {
                Change::Added(PythonVersion::PY39)
            }
            UnsupportedSyntaxErrorKind::PositionalOnlyParameter => {
                Change::Added(PythonVersion::PY38)
            }
            UnsupportedSyntaxErrorKind::ParenthesizedKeywordArgumentName => {
                Change::Removed(PythonVersion::PY38)
            }
            UnsupportedSyntaxErrorKind::TypeParameterList => Change::Added(PythonVersion::PY312),
            UnsupportedSyntaxErrorKind::TypeAliasStatement => Change::Added(PythonVersion::PY312),
            UnsupportedSyntaxErrorKind::TypeParamDefault => Change::Added(PythonVersion::PY313),
            UnsupportedSyntaxErrorKind::Pep701FString(_) => Change::Added(PythonVersion::PY312),
            UnsupportedSyntaxErrorKind::ParenthesizedContextManager => {
                Change::Added(PythonVersion::PY39)
            }
            UnsupportedSyntaxErrorKind::StarExpressionInIndex => {
                Change::Added(PythonVersion::PY311)
            }
            UnsupportedSyntaxErrorKind::StarAnnotation => Change::Added(PythonVersion::PY311),
            UnsupportedSyntaxErrorKind::UnparenthesizedUnpackInFor => {
                Change::Added(PythonVersion::PY39)
            }
        }
    }

    /// Returns whether or not this kind of syntax is unsupported on `target_version`.
    pub(crate) fn is_unsupported(self, target_version: PythonVersion) -> bool {
        match self.changed_version() {
            Change::Added(version) => target_version < version,
            Change::Removed(version) => target_version >= version,
        }
    }

    /// Returns `true` if this kind of syntax is supported on `target_version`.
    pub(crate) fn is_supported(self, target_version: PythonVersion) -> bool {
        !self.is_unsupported(target_version)
    }
}

#[cfg(target_pointer_width = "64")]
mod sizes {
    use crate::error::{LexicalError, LexicalErrorType};
    use static_assertions::assert_eq_size;

    assert_eq_size!(LexicalErrorType, [u8; 24]);
    assert_eq_size!(LexicalError, [u8; 32]);
}
