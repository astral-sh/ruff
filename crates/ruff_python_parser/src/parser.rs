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

use itertools::Itertools;
pub(super) use lalrpop_util::ParseError as LalrpopError;

use ruff_python_ast::{
    Expr, ExprAttribute, ExprAwait, ExprBinOp, ExprBoolOp, ExprBooleanLiteral, ExprBytesLiteral,
    ExprCall, ExprCompare, ExprDict, ExprDictComp, ExprEllipsisLiteral, ExprFString,
    ExprGeneratorExp, ExprIfExp, ExprIpyEscapeCommand, ExprLambda, ExprList, ExprListComp,
    ExprName, ExprNamedExpr, ExprNoneLiteral, ExprNumberLiteral, ExprSet, ExprSetComp, ExprSlice,
    ExprStarred, ExprStringLiteral, ExprSubscript, ExprTuple, ExprUnaryOp, ExprYield,
    ExprYieldFrom, Mod, ModModule, Suite,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::lexer::{lex, lex_starts_at, LexResult};
use crate::token_source::TokenSource;
use crate::{
    lexer::{self, LexicalError, LexicalErrorType},
    python,
    token::Tok,
    tokenize_all, Mode,
};

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
/// let program = parser::parse_program(source);
/// assert!(program.is_ok());
/// ```
pub fn parse_program(source: &str) -> Result<ModModule, ParseError> {
    match parse_tokens(tokenize_all(source, Mode::Module), source, Mode::Module)? {
        Mod::Module(m) => Ok(m),
        Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
    }
}

pub fn parse_suite(source: &str) -> Result<Suite, ParseError> {
    parse_program(source).map(|m| m.body)
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
/// let expr = parser::parse_expression("1 + 2");
///
/// assert!(expr.is_ok());
///
/// ```
pub fn parse_expression(source: &str) -> Result<Expr, ParseError> {
    let lexer = lex(source, Mode::Expression);
    match parse_tokens(lexer.collect(), source, Mode::Expression)? {
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
/// let expr = parse_expression_starts_at("1 + 2", TextSize::from(400));
/// assert!(expr.is_ok());
/// ```
pub fn parse_expression_starts_at(source: &str, offset: TextSize) -> Result<Expr, ParseError> {
    let lexer = lex_starts_at(source, Mode::Module, offset);
    match parse_tokens(lexer.collect(), source, Mode::Expression)? {
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
/// let expr = parse("1 + 2", Mode::Expression);
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
/// let program = parse(source, Mode::Module);
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
/// let program = parse(source, Mode::Ipython);
/// assert!(program.is_ok());
/// ```
pub fn parse(source: &str, mode: Mode) -> Result<Mod, ParseError> {
    parse_starts_at(source, mode, TextSize::default())
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
/// let program = parse_starts_at(source, Mode::Module, TextSize::from(0));
/// assert!(program.is_ok());
/// ```
pub fn parse_starts_at(source: &str, mode: Mode, offset: TextSize) -> Result<Mod, ParseError> {
    let lxr = lexer::lex_starts_at(source, mode, offset);
    parse_tokens(lxr.collect(), source, mode)
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
/// let expr = parse_tokens(lex(source, Mode::Expression).collect(), source, Mode::Expression);
/// assert!(expr.is_ok());
/// ```
pub fn parse_tokens(tokens: Vec<LexResult>, source: &str, mode: Mode) -> Result<Mod, ParseError> {
    let marker_token = (Tok::start_marker(mode), TextRange::default());
    let lexer = std::iter::once(Ok(marker_token)).chain(TokenSource::new(tokens));
    python::TopParser::new()
        .parse(
            source,
            mode,
            lexer.map_ok(|(t, range)| (range.start(), t, range.end())),
        )
        .map_err(parse_error_from_lalrpop)
}

/// Represents represent errors that occur during parsing and are
/// returned by the `parse_*` functions.

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub error: ParseErrorType,
    pub offset: TextSize,
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

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} at byte offset {}",
            &self.error,
            u32::from(self.offset)
        )
    }
}

/// Represents the different types of errors that can occur during parsing.
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
    // Maps to `User` type from `lalrpop-util`
    /// Parser encountered an error during lexing.
    Lexical(LexicalErrorType),
}

impl std::error::Error for ParseErrorType {}

// Convert `lalrpop_util::ParseError` to our internal type
fn parse_error_from_lalrpop(err: LalrpopError<TextSize, Tok, LexicalError>) -> ParseError {
    match err {
        // TODO: Are there cases where this isn't an EOF?
        LalrpopError::InvalidToken { location } => ParseError {
            error: ParseErrorType::Eof,
            offset: location,
        },
        LalrpopError::ExtraToken { token } => ParseError {
            error: ParseErrorType::ExtraToken(token.1),
            offset: token.0,
        },
        LalrpopError::User { error } => ParseError {
            error: ParseErrorType::Lexical(error.error),
            offset: error.location,
        },
        LalrpopError::UnrecognizedToken { token, expected } => {
            // Hacky, but it's how CPython does it. See PyParser_AddToken,
            // in particular "Only one possible expected token" comment.
            let expected = (expected.len() == 1).then(|| expected[0].clone());
            ParseError {
                error: ParseErrorType::UnrecognizedToken(token.1, expected),
                offset: token.0,
            }
        }
        LalrpopError::UnrecognizedEof { location, expected } => {
            // This could be an initial indentation error that we should ignore
            let indent_error = expected == ["Indent"];
            if indent_error {
                ParseError {
                    error: ParseErrorType::Lexical(LexicalErrorType::IndentationError),
                    offset: location,
                }
            } else {
                ParseError {
                    error: ParseErrorType::Eof,
                    offset: location,
                }
            }
        }
    }
}

impl std::fmt::Display for ParseErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
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
            ParseErrorType::Lexical(ref error) => write!(f, "{error}"),
        }
    }
}

impl ParseErrorType {
    /// Returns true if the error is an indentation error.
    pub fn is_indentation_error(&self) -> bool {
        match self {
            ParseErrorType::Lexical(LexicalErrorType::IndentationError) => true,
            ParseErrorType::UnrecognizedToken(token, expected) => {
                *token == Tok::Indent || expected.clone() == Some("Indent".to_owned())
            }
            _ => false,
        }
    }

    /// Returns true if the error is a tab error.
    pub fn is_tab_error(&self) -> bool {
        matches!(
            self,
            ParseErrorType::Lexical(LexicalErrorType::TabError | LexicalErrorType::TabsAfterSpaces)
        )
    }
}

impl From<LexicalError> for ParseError {
    fn from(error: LexicalError) -> Self {
        ParseError {
            error: ParseErrorType::Lexical(error.error),
            offset: error.location,
        }
    }
}

/// An expression that may be parenthesized.
#[derive(Clone, Debug)]
pub(super) struct ParenthesizedExpr {
    /// The range of the expression, including any parentheses.
    pub(super) range: TextRange,
    /// The underlying expression.
    pub(super) expr: Expr,
}

impl ParenthesizedExpr {
    /// Returns `true` if the expression is parenthesized.
    pub(super) fn is_parenthesized(&self) -> bool {
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
    use static_assertions::assert_eq_size;

    use crate::parser::ParenthesizedExpr;

    assert_eq_size!(ParenthesizedExpr, [u8; 88]);
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::*;

    #[test]
    fn test_parse_empty() {
        let parse_ast = parse_suite("").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string() {
        let source = "'Hello world'";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string() {
        let source = "f'Hello world'";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_hello() {
        let source = "print('Hello world')";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_2() {
        let source = "print('Hello world', 2)";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_kwargs() {
        let source = "my_func('positional', keyword=2)";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_elif_else() {
        let source = "if 1: 10\nelif 2: 20\nelse: 30";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_lambda() {
        let source = "lambda x, y: x * y"; // lambda(x, y): x * y";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_lambda_no_args() {
        let source = "lambda: 1";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_tuples() {
        let source = "a, b = 4, 5";

        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_class() {
        let source = "\
class Foo(A, B):
 def __init__(self):
  pass
 def method_with_default(self, arg='default'):
  pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_class_generic_types() {
        let source = "\
# TypeVar
class Foo[T](): ...

# TypeVar with bound
class Foo[T: str](): ...

# TypeVar with tuple bound
class Foo[T: (str, bytes)](): ...

# Multiple TypeVar
class Foo[T, U](): ...

# Trailing comma
class Foo[T, U,](): ...

# TypeVarTuple
class Foo[*Ts](): ...

# ParamSpec
class Foo[**P](): ...

# Mixed types
class Foo[X, Y: str, *U, **P]():
  pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }
    #[test]
    fn test_parse_function_definition() {
        let source = "\
def func(a):
    ...

def func[T](a: T) -> T:
    ...

def func[T: str](a: T) -> T:
    ...

def func[T: (str, bytes)](a: T) -> T:
    ...

def func[*Ts](*a: *Ts):
    ...

def func[**P](*args: P.args, **kwargs: P.kwargs):
    ...

def func[T, U: str, *Ts, **P]():
    pass
  ";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parse_dict_comprehension() {
        let source = "{x1: x2 for y in z}";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_list_comprehension() {
        let source = "[x for y in z]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_double_list_comprehension() {
        let source = "[x for y, y2 in z for a in b if a < 5 if a > 10]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_generator_comprehension() {
        let source = "(x for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_named_expression_generator_comprehension() {
        let source = "(x := y + 1 for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_else_generator_comprehension() {
        let source = "(x if y else y for y in z)";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_bool_op_or() {
        let source = "x or y";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_bool_op_and() {
        let source = "x and y";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_slice() {
        let source = "x[1:2:3]";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_named_expression() {
        let source = "(x := ( y * z ))";
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_with_statement() {
        let source = "\
with 0: pass
with 0 as x: pass
with 0, 1: pass
with 0 as x, 1 as y: pass
with 0 if 1 else 2: pass
with 0 if 1 else 2 as x: pass
with (): pass
with () as x: pass
with (0): pass
with (0) as x: pass
with (0,): pass
with (0,) as x: pass
with (0, 1): pass
with (0, 1) as x: pass
with (*a,): pass
with (*a,) as x: pass
with (0, *a): pass
with (0, *a) as x: pass
with (a := 0): pass
with (a := 0) as x: pass
with (a := 0, b := 1): pass
with (a := 0, b := 1) as x: pass
with (0 as a): pass
with (0 as a,): pass
with (0 as a, 1 as b): pass
with (0 as a, 1 as b,): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parenthesized_with_statement() {
        let source = "\
with ((a), (b)): pass
with ((a), (b), c as d, (e)): pass
with (a, b): pass
with (a, b) as c: pass
with ((a, b) as c): pass
with (a as b): pass
with (a): pass
with (a := 0): pass
with (a := 0) as x: pass
with ((a)): pass
with ((a := 0)): pass
with (a as b, (a := 0)): pass
with (a, (a := 0)): pass
with (yield): pass
with (yield from a): pass
with ((yield)): pass
with ((yield from a)): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_with_statement_invalid() {
        for source in [
            "with 0,: pass",
            "with 0 as x,: pass",
            "with 0 as *x: pass",
            "with *a: pass",
            "with *a as x: pass",
            "with (*a): pass",
            "with (*a) as x: pass",
            "with *a, 0 as x: pass",
            "with (*a, 0 as x): pass",
            "with 0 as x, *a: pass",
            "with (0 as x, *a): pass",
            "with (0 as x) as y: pass",
            "with (0 as x), 1: pass",
            "with ((0 as x)): pass",
            "with a := 0 as x: pass",
            "with (a := 0 as x): pass",
        ] {
            assert!(parse_suite(source).is_err());
        }
    }

    #[test]
    fn test_star_index() {
        let source = "\
array_slice = array[0, *indexes, -1]
array[0, *indexes, -1] = array_slice
array[*indexes_to_select, *indexes_to_select]
array[3:5, *indexes_to_select]
";
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_generator_expression_argument() {
        let source = r#"' '.join(
    sql
    for sql in (
        "LIMIT %d" % limit if limit else None,
        ("OFFSET %d" % offset) if offset else None,
    )
)"#;
        let parse_ast = parse_expression(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try() {
        let parse_ast = parse_suite(
            r"try:
    raise ValueError(1)
except TypeError as e:
    print(f'caught {type(e)}')
except OSError as e:
    print(f'caught {type(e)}')",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try_star() {
        let parse_ast = parse_suite(
            r#"try:
    raise ExceptionGroup("eg",
        [ValueError(1), TypeError(2), OSError(3), OSError(4)])
except* TypeError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')
except* OSError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')"#,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_dict_unpacking() {
        let parse_ast = parse_expression(r#"{"a": "b", **c, "d": "e"}"#).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_modes() {
        let source = "a[0][1][2][3][4]";

        assert!(parse(source, Mode::Expression).is_ok());
        assert!(parse(source, Mode::Module).is_ok());
    }

    #[test]
    fn test_parse_type_declaration() {
        let source = r#"
type X = int
type X = int | str
type X = int | "ForwardRefY"
type X[T] = T | list[X[T]]  # recursive
type X[T] = int
type X[T] = list[T] | set[T]
type X[T, *Ts, **P] = (T, Ts, P)
type X[T: int, *Ts, **P] = (T, Ts, P)
type X[T: (int, str), *Ts, **P] = (T, Ts, P)

# soft keyword as alias name
type type = int
type match = int
type case = int

# soft keyword as value
type foo = type
type foo = match
type foo = case

# multine definitions
type \
	X = int
type X \
	= int
type X = \
	int
type X = (
    int
)
type \
    X[T] = T
type X \
    [T] = T
type X[T] \
    = T

# simple statements
type X = int; type X = str; type X = type
class X: type X = int
"#;
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_type_as_identifier() {
        let source = r"\
type *a + b, c   # ((type * a) + b), c
type *(a + b), c   # (type * (a + b)), c
type (*a + b, c)   # type ((*(a + b)), c)
type -a * b + c   # (type - (a * b)) + c
type -(a * b) + c   # (type - (a * b)) + c
type (-a) * b + c   # (type (-(a * b))) + c
type ().a   # (type()).a
type (()).a   # (type(())).a
type ((),).a   # (type(())).a
type [a].b   # (type[a]).b
type [a,].b   # (type[(a,)]).b  (not (type[a]).b)
type [(a,)].b   # (type[(a,)]).b
type()[a:
    b]  # (type())[a: b]
if type := 1: pass
type = lambda query: query == event
print(type(12))
type(type)
a = (
	type in C
)
a = (
	type(b)
)
type (
	X = int
)
type = 1
type = x = 1
x = type = 1
lambda x: type
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_invalid_type() {
        assert!(parse_suite("a: type X = int").is_err());
        assert!(parse_suite("lambda: type X = int").is_err());
    }

    #[test]
    fn numeric_literals() {
        let source = r"x = 123456789
x = 123456
x = .1
x = 1.
x = 1E+1
x = 1E-1
x = 1.000_000_01
x = 123456789.123456789
x = 123456789.123456789E123456789
x = 123456789E123456789
x = 123456789J
x = 123456789.123456789J
x = 0XB1ACC
x = 0B1011
x = 0O777
x = 0.000000006
x = 10000
x = 133333
";

        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn numeric_literals_attribute_access() {
        let source = r"x = .1.is_integer()
x = 1. .imag
x = 1E+1.imag
x = 1E-1.real
x = 123456789.123456789.hex()
x = 123456789.123456789E123456789 .real
x = 123456789E123456789 .conjugate()
x = 123456789J.real
x = 123456789.123456789J.__add__(0b1011.bit_length())
x = 0XB1ACC.conjugate()
x = 0B1011 .conjugate()
x = 0O777 .real
x = 0.000000006  .hex()
x = -100.0000J

if 10 .real:
    ...

y = 100[no]
y = 100(no)
";
        assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_match_as_identifier() {
        let source = r"\
match *a + b, c   # ((match * a) + b), c
match *(a + b), c   # (match * (a + b)), c
match (*a + b, c)   # match ((*(a + b)), c)
match -a * b + c   # (match - (a * b)) + c
match -(a * b) + c   # (match - (a * b)) + c
match (-a) * b + c   # (match (-(a * b))) + c
match ().a   # (match()).a
match (()).a   # (match(())).a
match ((),).a   # (match(())).a
match [a].b   # (match[a]).b
match [a,].b   # (match[(a,)]).b  (not (match[a]).b)
match [(a,)].b   # (match[(a,)]).b
match()[a:
    b]  # (match())[a: b]
if match := 1: pass
match match:
    case 1: pass
    case 2:
        pass
match = lambda query: query == event
print(match(12))
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_patma() {
        let source = r#"# Cases sampled from Lib/test/test_patma.py

# case test_patma_098
match x:
    case -0j:
        y = 0
# case test_patma_142
match x:
    case bytes(z):
        y = 0
# case test_patma_073
match x:
    case 0 if 0:
        y = 0
    case 0 if 1:
        y = 1
# case test_patma_006
match 3:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_049
match x:
    case [0, 1] | [1, 0]:
        y = 0
# case black_check_sequence_then_mapping
match x:
    case [*_]:
        return "seq"
    case {}:
        return "map"
# case test_patma_035
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | True} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_107
match x:
    case 0.25 + 1.75j:
        y = 0
# case test_patma_097
match x:
    case -0j:
        y = 0
# case test_patma_007
match 4:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_154
match x:
    case 0 if x:
        y = 0
# case test_patma_134
match x:
    case {1: 0}:
        y = 0
    case {0: 0}:
        y = 1
    case {**z}:
        y = 2
# case test_patma_185
match Seq():
    case [*_]:
        y = 0
# case test_patma_063
match x:
    case 1:
        y = 0
    case 1:
        y = 1
# case test_patma_248
match x:
    case {"foo": bar}:
        y = bar
# case test_patma_019
match (0, 1, 2):
    case [0, 1, *x, 2]:
        y = 0
# case test_patma_052
match x:
    case [0]:
        y = 0
    case [1, 0] if (x := x[:0]):
        y = 1
    case [1, 0]:
        y = 2
# case test_patma_191
match w:
    case [x, y, *_]:
        z = 0
# case test_patma_110
match x:
    case -0.25 - 1.75j:
        y = 0
# case test_patma_151
match (x,):
    case [y]:
        z = 0
# case test_patma_114
match x:
    case A.B.C.D:
        y = 0
# case test_patma_232
match x:
    case None:
        y = 0
# case test_patma_058
match x:
    case 0:
        y = 0
# case test_patma_233
match x:
    case False:
        y = 0
# case test_patma_078
match x:
    case []:
        y = 0
    case [""]:
        y = 1
    case "":
        y = 2
# case test_patma_156
match x:
    case z:
        y = 0
# case test_patma_189
match w:
    case [x, y, *rest]:
        z = 0
# case test_patma_042
match x:
    case (0 as z) | (1 as z) | (2 as z) if z == x % 2:
        y = 0
# case test_patma_034
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | False} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_123
match (0, 1, 2):
    case 0, *x:
        y = 0
# case test_patma_126
match (0, 1, 2):
    case *x, 2,:
        y = 0
# case test_patma_151
match x,:
    case y,:
        z = 0
# case test_patma_152
match w, x:
    case y, z:
        v = 0
# case test_patma_153
match w := x,:
    case y as v,:
        z = 0
"#;
        let parse_ast = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_match() {
        let parse_ast = parse_suite(
            r#"
match {"test": 1}:
    case {
        **rest,
    }:
        print(rest)
match {"label": "test"}:
    case {
        "label": str() | None as label,
    }:
        print(label)
match x:
    case [0, 1,]:
        y = 0
match x:
    case (0, 1,):
        y = 0
match x:
    case (0,):
        y = 0
match x,:
    case z:
        pass
match x, y:
    case z:
        pass
match x, y,:
    case z:
        pass
"#,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_match_pattern_fstring_literal() {
        // F-string literal is not allowed in match pattern.
        let parse_error = parse_suite(
            r#"
match x:
    case f"{y}":
        pass
"#,
        )
        .err();
        assert!(
            parse_error.is_some(),
            "expected parse error when f-string literal is used in match pattern"
        );
    }

    #[test]
    fn test_variadic_generics() {
        let parse_ast = parse_suite(
            r"
def args_to_tuple(*args: *Ts) -> Tuple[*Ts]: ...
",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn decorator_ranges() {
        let parse_ast = parse_suite(
            r"
@my_decorator
def test():
    pass

@class_decorator
class Abcd:
    pass
"
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_ipython_escape_commands() {
        let parse_ast = parse(
            r"
# Normal Python code
(
    a
    %
    b
)

# Dynamic object info
??a.foo
?a.foo
?a.foo?
??a.foo()??

# Line magic
%timeit a = b
%timeit foo(b) % 3
%alias showPath pwd && ls -a
%timeit a =\
  foo(b); b = 2
%matplotlib --inline
%matplotlib \
    --inline

# System shell access
!pwd && ls -a | sed 's/^/\    /'
!pwd \
  && ls -a | sed 's/^/\\    /'
!!cd /Users/foo/Library/Application\ Support/

# Let's add some Python code to make sure that earlier escapes were handled
# correctly and that we didn't consume any of the following code as a result
# of the escapes.
def foo():
    return (
        a
        !=
        b
    )

# Transforms into `foo(..)`
/foo 1 2
;foo 1 2
,foo 1 2

# Indented escape commands
for a in range(5):
    !ls

p1 = !pwd
p2: str = !pwd
foo = %foo \
    bar

% foo
foo = %foo  # comment

# Help end line magics
foo?
foo.bar??
foo.bar.baz?
foo[0]??
foo[0][1]?
foo.bar[0].baz[1]??
foo.bar[0].baz[2].egg??
"
            .trim(),
            Mode::Ipython,
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_ipython_escape_command_parse_error() {
        let source = r"
a = 1
%timeit a == 1
    "
        .trim();
        let lxr = lexer::lex_starts_at(source, Mode::Ipython, TextSize::default());
        let parse_err = parse_tokens(lxr.collect(), source, Mode::Module).unwrap_err();
        assert_eq!(
            parse_err.to_string(),
            "IPython escape commands are only allowed in `Mode::Ipython` at byte offset 6"
                .to_string()
        );
    }

    #[test]
    fn test_fstrings() {
        let parse_ast = parse_suite(
            r#"
f"{" f"}"
f"{foo!s}"
f"{3,}"
f"{3!=4:}"
f'{3:{"}"}>10}'
f'{3:{"{"}>10}'
f"{  foo =  }"
f"{  foo =  :.3f  }"
f"{  foo =  !s  }"
f"{  1, 2  =  }"
f'{f"{3.1415=:.1f}":*^20}'

{"foo " f"bar {x + y} " "baz": 10}
match foo:
    case "one":
        pass
    case "implicitly " "concatenated":
        pass

f"\{foo}\{bar:\}"
f"\\{{foo\\}}"
f"""{
    foo:x
        y
        z
}"""
"#
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstrings_with_unicode() {
        let parse_ast = parse_suite(
            r#"
u"foo" f"{bar}" "baz" " some"
"foo" f"{bar}" u"baz" " some"
"foo" f"{bar}" "baz" u" some"
u"foo" f"bar {baz} really" u"bar" "no"
"#
            .trim(),
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_unicode_aliases() {
        // https://github.com/RustPython/RustPython/issues/4566
        let parse_ast = parse_suite(r#"x = "\N{BACKSPACE}another cool trick""#).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
