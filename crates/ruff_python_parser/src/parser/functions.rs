//! Contains the interface to the Python `ruff_python_parser`.
//!
//! Functions in this module can be used to parse Python code into an [Abstract Syntax Tree]
//! (AST) that is then transformed into bytecode.
//!
//! There are three ways to parse Python code corresponding to the different [`Mode`]s
//! defined in the [`mode`] module.
//!
//! All functions return a [`Result`](std::result::Result) containing the parsed AST or
//! a [`ParseError`] if parsing failed.
//!
//! [Abstract Syntax Tree]: https://en.wikipedia.org/wiki/Abstract_syntax_tree
//! [`Mode`]: crate::mode

use std::iter;

use itertools::Itertools;
pub(super) use lalrpop_util::ParseError as LalrpopError;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::lexer::{lex, lex_starts_at, LexicalError, LexicalErrorType, Spanned};
use crate::{
    lexer::{self, LexResult},
    token::Tok,
    Mode,
};
use crate::{python, ParseError, ParseErrorType};
use ruff_python_ast::{
    Expr, ExprAttribute, ExprAwait, ExprBinOp, ExprBoolOp, ExprBooleanLiteral, ExprBytesLiteral,
    ExprCall, ExprCompare, ExprDict, ExprDictComp, ExprEllipsisLiteral, ExprFString,
    ExprGeneratorExp, ExprIfExp, ExprIpyEscapeCommand, ExprLambda, ExprList, ExprListComp,
    ExprName, ExprNamedExpr, ExprNoneLiteral, ExprNumberLiteral, ExprSet, ExprSetComp, ExprSlice,
    ExprStarred, ExprStringLiteral, ExprSubscript, ExprTuple, ExprUnaryOp, ExprYield,
    ExprYieldFrom, Mod, ModModule, Suite,
};

use super::Parser;

/// Parse a full Python program usually consisting of multiple lines.
///
/// This is a convenience function that can be used to parse a full Python program without having to
/// specify the [`Mode`] or the location. It is probably what you want to use most of the time.
///
/// # Example
///
/// For example, parsing a simple function definition and a call to that function:
///
/// ```
/// use ruff_python_parser as parser;
/// let source = r#"
/// def foo():
///    return 42
///
/// print(foo())
/// "#;
/// let program = parser::parse_program(source, "<embedded>");
/// assert!(program.is_ok());
/// ```
pub fn parse_program(source: &str, source_path: &str) -> Result<ModModule, ParseError> {
    let lexer = lex(source, Mode::Module);
    match parse_tokens(lexer, source, Mode::Module, source_path)? {
        Mod::Module(m) => Ok(m),
        Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
    }
}

pub fn parse_suite(source: &str, source_path: &str) -> Result<Suite, ParseError> {
    parse_program(source, source_path).map(|m| m.body)
}

/// Parses a single Python expression.
///
/// This convenience function can be used to parse a single expression without having to
/// specify the Mode or the location.
///
/// # Example
///
/// For example, parsing a single expression denoting the addition of two numbers:
///
///  ```
/// use ruff_python_parser as parser;
/// let expr = parser::parse_expression("1 + 2", "<embedded>");
///
/// assert!(expr.is_ok());
///
/// ```
pub fn parse_expression(source: &str, source_path: &str) -> Result<Expr, ParseError> {
    let lexer = lex(source, Mode::Expression);
    match parse_tokens(lexer, source, Mode::Expression, source_path)? {
        Mod::Expression(expression) => Ok(*expression.body),
        Mod::Module(_m) => unreachable!("Mode::Expression doesn't return other variant"),
    }
}

/// Parses a Python expression from a given location.
///
/// This function allows to specify the location of the expression in the source code, other than
/// that, it behaves exactly like [`parse_expression`].
///
/// # Example
///
/// Parsing a single expression denoting the addition of two numbers, but this time specifying a different,
/// somewhat silly, location:
///
/// ```
/// use ruff_python_parser::{parse_expression_starts_at};
/// # use ruff_text_size::TextSize;
///
/// let expr = parse_expression_starts_at("1 + 2", "<embedded>", TextSize::from(400));
/// assert!(expr.is_ok());
/// ```
pub fn parse_expression_starts_at(
    source: &str,
    source_path: &str,
    offset: TextSize,
) -> Result<Expr, ParseError> {
    let lexer = lex_starts_at(source, Mode::Module, offset);
    match parse_tokens(lexer, source, Mode::Expression, source_path)? {
        Mod::Expression(expression) => Ok(*expression.body),
        Mod::Module(_m) => unreachable!("Mode::Expression doesn't return other variant"),
    }
}

/// Parse the given Python source code using the specified [`Mode`].
///
/// This function is the most general function to parse Python code. Based on the [`Mode`] supplied,
/// it can be used to parse a single expression, a full Python program, an interactive expression
/// or a Python program containing IPython escape commands.
///
/// # Example
///
/// If we want to parse a simple expression, we can use the [`Mode::Expression`] mode during
/// parsing:
///
/// ```
/// use ruff_python_parser::{Mode, parse};
///
/// let expr = parse("1 + 2", Mode::Expression, "<embedded>");
/// assert!(expr.is_ok());
/// ```
///
/// Alternatively, we can parse a full Python program consisting of multiple lines:
///
/// ```
/// use ruff_python_parser::{Mode, parse};
///
/// let source = r#"
/// class Greeter:
///
///   def greet(self):
///    print("Hello, world!")
/// "#;
/// let program = parse(source, Mode::Module, "<embedded>");
/// assert!(program.is_ok());
/// ```
///
/// Additionally, we can parse a Python program containing IPython escapes:
///
/// ```
/// use ruff_python_parser::{Mode, parse};
///
/// let source = r#"
/// %timeit 1 + 2
/// ?str.replace
/// !ls
/// "#;
/// let program = parse(source, Mode::Ipython, "<embedded>");
/// assert!(program.is_ok());
/// ```
pub fn parse(source: &str, mode: Mode, source_path: &str) -> Result<Mod, ParseError> {
    parse_starts_at(source, mode, source_path, TextSize::default())
}

/// Parse the given Python source code using the specified [`Mode`] and [`TextSize`].
///
/// This function allows to specify the location of the the source code, other than
/// that, it behaves exactly like [`parse`].
///
/// # Example
///
/// ```
/// # use ruff_text_size::TextSize;
/// use ruff_python_parser::{Mode, parse_starts_at};
///
/// let source = r#"
/// def fib(i):
///    a, b = 0, 1
///    for _ in range(i):
///       a, b = b, a + b
///    return a
///
/// print(fib(42))
/// "#;
/// let program = parse_starts_at(source, Mode::Module, "<embedded>", TextSize::from(0));
/// assert!(program.is_ok());
/// ```
pub fn parse_starts_at(
    source: &str,
    mode: Mode,
    source_path: &str,
    offset: TextSize,
) -> Result<Mod, ParseError> {
    let lxr = lexer::lex_starts_at(source, mode, offset);
    parse_tokens(lxr, source, mode, source_path)
}

/// Parse an iterator of [`LexResult`]s using the specified [`Mode`].
///
/// This could allow you to perform some preprocessing on the tokens before parsing them.
///
/// # Example
///
/// As an example, instead of parsing a string, we can parse a list of tokens after we generate
/// them using the [`lexer::lex`] function:
///
/// ```
/// use ruff_python_parser::{lexer::lex, Mode, parse_tokens};
///
/// let source = "1 + 2";
/// let expr = parse_tokens(lex(source, Mode::Expression), source, Mode::Expression, "<embedded>");
/// assert!(expr.is_ok());
/// ```
pub fn parse_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    source: &str,
    mode: Mode,
    source_path: &str,
) -> Result<Mod, ParseError> {
    let lxr = lxr.into_iter();

    parse_filtered_tokens(
        lxr.filter_ok(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline)),
        source,
        mode,
        source_path,
    )
}

pub fn parse_ok_tokens_new(
    lxr: impl IntoIterator<Item = Spanned>,
    source: &str,
    mode: Mode,
    source_path: &str,
) -> Result<Mod, ParseError> {
    let lxr = lxr
        .into_iter()
        .filter(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline))
        .map(Ok::<(Tok, TextRange), LexicalError>);
    let parsed_file = Parser::new(source, source_path, mode, lxr).parse();
    if parsed_file.parse_errors.is_empty() {
        Ok(parsed_file.ast)
    } else {
        Err(parsed_file.parse_errors.into_iter().next().unwrap())
    }
}

pub fn parse_ok_tokens_lalrpop(
    lxr: impl IntoIterator<Item = Spanned>,
    source: &str,
    mode: Mode,
    source_path: &str,
) -> Result<Mod, ParseError> {
    let lxr = lxr
        .into_iter()
        .filter(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline));
    let marker_token = (Tok::start_marker(mode), TextRange::default());
    let lexer = iter::once(marker_token)
        .chain(lxr)
        .map(|(t, range)| (range.start(), t, range.end()));
    python::TopParser::new()
        .parse(source, mode, lexer)
        .map_err(|e| parse_error_from_lalrpop(e, source_path))
}

/// Parse tokens into an AST like [`parse_tokens`], but we already know all tokens are valid.
pub fn parse_ok_tokens(
    lxr: impl IntoIterator<Item = Spanned>,
    source: &str,
    mode: Mode,
    source_path: &str,
) -> Result<Mod, ParseError> {
    if std::env::var("NEW_PARSER").is_ok() {
        parse_ok_tokens_new(lxr, source, mode, source_path)
    } else {
        parse_ok_tokens_lalrpop(lxr, source, mode, source_path)
    }
}

fn parse_filtered_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    source: &str,
    mode: Mode,
    source_path: &str,
) -> Result<Mod, ParseError> {
    if std::env::var("NEW_PARSER").is_ok() {
        let parsed_file = Parser::new(source, source_path, mode, lxr.into_iter()).parse();
        if parsed_file.parse_errors.is_empty() {
            Ok(parsed_file.ast)
        } else {
            Err(parsed_file.parse_errors.into_iter().next().unwrap())
        }
    } else {
        let marker_token = (Tok::start_marker(mode), TextRange::default());
        let lexer = iter::once(Ok(marker_token)).chain(lxr);
        python::TopParser::new()
            .parse(
                source,
                mode,
                lexer.map_ok(|(t, range)| (range.start(), t, range.end())),
            )
            .map_err(|e| parse_error_from_lalrpop(e, source_path))
    }
}

fn parse_error_from_lalrpop(
    err: LalrpopError<TextSize, Tok, LexicalError>,
    source_path: &str,
) -> ParseError {
    let source_path = source_path.to_owned();

    match err {
        // TODO: Are there cases where this isn't an EOF?
        LalrpopError::InvalidToken { location } => ParseError {
            error: ParseErrorType::Eof,
            location: TextRange::empty(location),
            source_path,
        },
        LalrpopError::ExtraToken { token } => ParseError {
            error: ParseErrorType::ExtraToken(token.1),
            location: TextRange::new(token.0, token.2),
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
                location: TextRange::new(token.0, token.2),
                source_path,
            }
        }
        LalrpopError::UnrecognizedEof { location, expected } => {
            // This could be an initial indentation error that we should ignore
            let indent_error = expected == ["Indent"];
            if indent_error {
                ParseError {
                    error: ParseErrorType::Lexical(LexicalErrorType::IndentationError),
                    location: TextRange::empty(location),
                    source_path,
                }
            } else {
                ParseError {
                    error: ParseErrorType::Eof,
                    location: TextRange::empty(location),
                    source_path,
                }
            }
        }
    }
}

/// An expression that may be parenthesized.
#[derive(Clone, Debug)]
pub struct ParenthesizedExpr {
    /// The range of the expression, including any parentheses.
    pub range: TextRange,
    /// The underlying expression.
    pub expr: Expr,
}

impl ParenthesizedExpr {
    /// Returns `true` if the expression is parenthesized.
    pub fn is_parenthesized(&self) -> bool {
        self.range.start() != self.expr.range().start()
    }
}

impl Ranged for ParenthesizedExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl From<Expr> for ParenthesizedExpr {
    fn from(expr: Expr) -> Self {
        ParenthesizedExpr {
            range: expr.range(),
            expr,
        }
    }
}
impl From<ParenthesizedExpr> for Expr {
    fn from(parenthesized_expr: ParenthesizedExpr) -> Self {
        parenthesized_expr.expr
    }
}
impl From<ExprIpyEscapeCommand> for ParenthesizedExpr {
    fn from(payload: ExprIpyEscapeCommand) -> Self {
        Expr::IpyEscapeCommand(payload).into()
    }
}
impl From<ExprBoolOp> for ParenthesizedExpr {
    fn from(payload: ExprBoolOp) -> Self {
        Expr::BoolOp(payload).into()
    }
}
impl From<ExprNamedExpr> for ParenthesizedExpr {
    fn from(payload: ExprNamedExpr) -> Self {
        Expr::NamedExpr(payload).into()
    }
}
impl From<ExprBinOp> for ParenthesizedExpr {
    fn from(payload: ExprBinOp) -> Self {
        Expr::BinOp(payload).into()
    }
}
impl From<ExprUnaryOp> for ParenthesizedExpr {
    fn from(payload: ExprUnaryOp) -> Self {
        Expr::UnaryOp(payload).into()
    }
}
impl From<ExprLambda> for ParenthesizedExpr {
    fn from(payload: ExprLambda) -> Self {
        Expr::Lambda(payload).into()
    }
}
impl From<ExprIfExp> for ParenthesizedExpr {
    fn from(payload: ExprIfExp) -> Self {
        Expr::IfExp(payload).into()
    }
}
impl From<ExprDict> for ParenthesizedExpr {
    fn from(payload: ExprDict) -> Self {
        Expr::Dict(payload).into()
    }
}
impl From<ExprSet> for ParenthesizedExpr {
    fn from(payload: ExprSet) -> Self {
        Expr::Set(payload).into()
    }
}
impl From<ExprListComp> for ParenthesizedExpr {
    fn from(payload: ExprListComp) -> Self {
        Expr::ListComp(payload).into()
    }
}
impl From<ExprSetComp> for ParenthesizedExpr {
    fn from(payload: ExprSetComp) -> Self {
        Expr::SetComp(payload).into()
    }
}
impl From<ExprDictComp> for ParenthesizedExpr {
    fn from(payload: ExprDictComp) -> Self {
        Expr::DictComp(payload).into()
    }
}
impl From<ExprGeneratorExp> for ParenthesizedExpr {
    fn from(payload: ExprGeneratorExp) -> Self {
        Expr::GeneratorExp(payload).into()
    }
}
impl From<ExprAwait> for ParenthesizedExpr {
    fn from(payload: ExprAwait) -> Self {
        Expr::Await(payload).into()
    }
}
impl From<ExprYield> for ParenthesizedExpr {
    fn from(payload: ExprYield) -> Self {
        Expr::Yield(payload).into()
    }
}
impl From<ExprYieldFrom> for ParenthesizedExpr {
    fn from(payload: ExprYieldFrom) -> Self {
        Expr::YieldFrom(payload).into()
    }
}
impl From<ExprCompare> for ParenthesizedExpr {
    fn from(payload: ExprCompare) -> Self {
        Expr::Compare(payload).into()
    }
}
impl From<ExprCall> for ParenthesizedExpr {
    fn from(payload: ExprCall) -> Self {
        Expr::Call(payload).into()
    }
}
impl From<ExprFString> for ParenthesizedExpr {
    fn from(payload: ExprFString) -> Self {
        Expr::FString(payload).into()
    }
}
impl From<ExprStringLiteral> for ParenthesizedExpr {
    fn from(payload: ExprStringLiteral) -> Self {
        Expr::StringLiteral(payload).into()
    }
}
impl From<ExprBytesLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBytesLiteral) -> Self {
        Expr::BytesLiteral(payload).into()
    }
}
impl From<ExprNumberLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNumberLiteral) -> Self {
        Expr::NumberLiteral(payload).into()
    }
}
impl From<ExprBooleanLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBooleanLiteral) -> Self {
        Expr::BooleanLiteral(payload).into()
    }
}
impl From<ExprNoneLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNoneLiteral) -> Self {
        Expr::NoneLiteral(payload).into()
    }
}
impl From<ExprEllipsisLiteral> for ParenthesizedExpr {
    fn from(payload: ExprEllipsisLiteral) -> Self {
        Expr::EllipsisLiteral(payload).into()
    }
}
impl From<ExprAttribute> for ParenthesizedExpr {
    fn from(payload: ExprAttribute) -> Self {
        Expr::Attribute(payload).into()
    }
}
impl From<ExprSubscript> for ParenthesizedExpr {
    fn from(payload: ExprSubscript) -> Self {
        Expr::Subscript(payload).into()
    }
}
impl From<ExprStarred> for ParenthesizedExpr {
    fn from(payload: ExprStarred) -> Self {
        Expr::Starred(payload).into()
    }
}
impl From<ExprName> for ParenthesizedExpr {
    fn from(payload: ExprName) -> Self {
        Expr::Name(payload).into()
    }
}
impl From<ExprList> for ParenthesizedExpr {
    fn from(payload: ExprList) -> Self {
        Expr::List(payload).into()
    }
}
impl From<ExprTuple> for ParenthesizedExpr {
    fn from(payload: ExprTuple) -> Self {
        Expr::Tuple(payload).into()
    }
}
impl From<ExprSlice> for ParenthesizedExpr {
    fn from(payload: ExprSlice) -> Self {
        Expr::Slice(payload).into()
    }
}

#[cfg(target_pointer_width = "64")]
mod size_assertions {
    use crate::parser::ParenthesizedExpr;
    use static_assertions::assert_eq_size;

    assert_eq_size!(ParenthesizedExpr, [u8; 88]);
}
