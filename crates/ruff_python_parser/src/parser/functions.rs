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

use ruff_python_ast::{Expr, Mod, ModModule, Suite};
use ruff_text_size::{TextRange, TextSize};

use crate::lexer::{lex, lex_starts_at, LexicalError, Spanned};
use crate::ParseError;
use crate::{
    lexer::{self, LexResult},
    token::Tok,
    Mode,
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
/// let program = parser::parse_program(source);
/// assert!(program.is_ok());
/// ```
pub fn parse_program(source: &str) -> Result<ModModule, ParseError> {
    let lexer = lex(source, Mode::Module);
    match parse_tokens(lexer, source, Mode::Module)? {
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
    match parse_tokens(lexer, source, Mode::Expression)? {
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
    match parse_tokens(lexer, source, Mode::Expression)? {
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
    parse_tokens(lxr, source, mode)
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
/// let expr = parse_tokens(lex(source, Mode::Expression), source, Mode::Expression);
/// assert!(expr.is_ok());
/// ```
pub fn parse_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    let lxr = lxr.into_iter();

    parse_filtered_tokens(
        lxr.filter_ok(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline)),
        source,
        mode,
    )
}

fn parse_ok_tokens_new(
    lxr: impl IntoIterator<Item = Spanned>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    let lxr = lxr
        .into_iter()
        .filter(|(tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline))
        .map(Ok::<(Tok, TextRange), LexicalError>);
    let parsed_file = Parser::new(source, mode, lxr).parse();
    if parsed_file.parse_errors.is_empty() {
        Ok(parsed_file.ast)
    } else {
        Err(parsed_file.parse_errors.into_iter().next().unwrap())
    }
}

/// Parse tokens into an AST like [`parse_tokens`], but we already know all tokens are valid.
pub fn parse_ok_tokens(
    lxr: impl IntoIterator<Item = Spanned>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    if std::env::var("NEW_PARSER").is_ok() {
        parse_ok_tokens_new(lxr, source, mode)
    } else {
        crate::lalrpop::parse_ok_tokens(lxr, source, mode)
    }
}

fn parse_filtered_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    if std::env::var("NEW_PARSER").is_ok() {
        let parsed_file = Parser::new(source, mode, lxr.into_iter()).parse();
        if parsed_file.parse_errors.is_empty() {
            Ok(parsed_file.ast)
        } else {
            Err(parsed_file.parse_errors.into_iter().next().unwrap())
        }
    } else {
        crate::lalrpop::parse_filtered_tokens(lxr, source, mode)
    }
}
