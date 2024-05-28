//! This crate can be used to parse Python source code into an Abstract
//! Syntax Tree.
//!
//! ## Overview
//!
//! The process by which source code is parsed into an AST can be broken down
//! into two general stages: [lexical analysis] and [parsing].
//!
//! During lexical analysis, the source code is converted into a stream of lexical
//! tokens that represent the smallest meaningful units of the language. For example,
//! the source code `print("Hello world")` would _roughly_ be converted into the following
//! stream of tokens:
//!
//! ```text
//! Name("print"), LeftParen, String("Hello world"), RightParen
//! ```
//!
//! These tokens are then consumed by the `ruff_python_parser`, which matches them against a set of
//! grammar rules to verify that the source code is syntactically valid and to construct
//! an AST that represents the source code.
//!
//! During parsing, the `ruff_python_parser` consumes the tokens generated by the lexer and constructs
//! a tree representation of the source code. The tree is made up of nodes that represent
//! the different syntactic constructs of the language. If the source code is syntactically
//! invalid, parsing fails and an error is returned. After a successful parse, the AST can
//! be used to perform further analysis on the source code. Continuing with the example
//! above, the AST generated by the `ruff_python_parser` would _roughly_ look something like this:
//!
//! ```text
//! node: Expr {
//!     value: {
//!         node: Call {
//!             func: {
//!                 node: Name {
//!                     id: "print",
//!                     ctx: Load,
//!                 },
//!             },
//!             args: [
//!                 node: Constant {
//!                     value: Str("Hello World"),
//!                     kind: None,
//!                 },
//!             ],
//!             keywords: [],
//!         },
//!     },
//! },
//!```
//!
//! **Note:** The Tokens/ASTs shown above are not the exact tokens/ASTs generated by the `ruff_python_parser`.
//! Refer to the [playground](https://play.ruff.rs) for the correct representation.
//!
//! ## Source code layout
//!
//! The functionality of this crate is split into several modules:
//!
//! - token: This module contains the definition of the tokens that are generated by the lexer.
//! - [lexer]: This module contains the lexer and is responsible for generating the tokens.
//! - parser: This module contains an interface to the [Program] and is responsible for generating the AST.
//! - mode: This module contains the definition of the different modes that the `ruff_python_parser` can be in.
//!
//! # Examples
//!
//! For example, to get a stream of tokens from a given string, one could do this:
//!
//! ```
//! use ruff_python_parser::{lexer::lex, Mode};
//!
//! let python_source = r#"
//! def is_odd(i):
//!     return bool(i & 1)
//! "#;
//! let mut tokens = lex(python_source, Mode::Module);
//! assert!(tokens.all(|t| t.is_ok()));
//! ```
//!
//! These tokens can be directly fed into the `ruff_python_parser` to generate an AST:
//!
//! ```
//! use ruff_python_parser::lexer::lex;
//! use ruff_python_parser::{Mode, parse_tokens};
//!
//! let python_source = r#"
//! def is_odd(i):
//!    return bool(i & 1)
//! "#;
//! let tokens = lex(python_source, Mode::Module);
//! let ast = parse_tokens(tokens.collect(), python_source, Mode::Module);
//!
//! assert!(ast.is_ok());
//! ```
//!
//! Alternatively, you can use one of the other `parse_*` functions to parse a string directly without using a specific
//! mode or tokenizing the source beforehand:
//!
//! ```
//! use ruff_python_parser::parse_suite;
//!
//! let python_source = r#"
//! def is_odd(i):
//!   return bool(i & 1)
//! "#;
//! let ast = parse_suite(python_source);
//!
//! assert!(ast.is_ok());
//! ```
//!
//! [lexical analysis]: https://en.wikipedia.org/wiki/Lexical_analysis
//! [parsing]: https://en.wikipedia.org/wiki/Parsing
//! [lexer]: crate::lexer

use std::iter::FusedIterator;
use std::ops::Deref;

use ruff_python_ast::{Expr, Mod, ModModule, PySourceType, Suite};
use ruff_text_size::{TextRange, TextSize};

pub use crate::error::{FStringErrorType, ParseError, ParseErrorType};
use crate::lexer::{lex, lex_starts_at, LexResult};
pub use crate::parser::Program;
pub use crate::token::{Tok, TokenKind};

mod error;
pub mod lexer;
mod parser;
mod soft_keywords;
mod string;
mod token;
mod token_set;
mod token_source;
pub mod typing;

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
/// use ruff_python_parser::parse_program;
///
/// let source = r#"
/// def foo():
///    return 42
///
/// print(foo())
/// "#;
///
/// let program = parse_program(source);
/// assert!(program.is_ok());
/// ```
pub fn parse_program(source: &str) -> Result<ModModule, ParseError> {
    let lexer = lex(source, Mode::Module);
    match parse_tokens(lexer.collect(), source, Mode::Module)? {
        Mod::Module(m) => Ok(m),
        Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
    }
}

/// Parse a full Python program into a [`Suite`].
///
/// This function is similar to [`parse_program`] except that it returns the module body
/// instead of the module itself.
///
/// # Example
///
/// For example, parsing a simple function definition and a call to that function:
///
/// ```
/// use ruff_python_parser::parse_suite;
///
/// let source = r#"
/// def foo():
///    return 42
///
/// print(foo())
/// "#;
///
/// let body = parse_suite(source);
/// assert!(body.is_ok());
/// ```
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
/// ```
/// use ruff_python_parser::parse_expression;
///
/// let expr = parse_expression("1 + 2");
/// assert!(expr.is_ok());
/// ```
pub fn parse_expression(source: &str) -> Result<Expr, ParseError> {
    let lexer = lex(source, Mode::Expression).collect();
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
/// use ruff_python_parser::parse_expression_starts_at;
/// # use ruff_text_size::TextSize;
///
/// let expr = parse_expression_starts_at("1 + 2", TextSize::from(400));
/// assert!(expr.is_ok());
/// ```
pub fn parse_expression_starts_at(source: &str, offset: TextSize) -> Result<Expr, ParseError> {
    let lexer = lex_starts_at(source, Mode::Module, offset).collect();
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
    let lxr = lexer::lex(source, mode);
    parse_tokens(lxr.collect(), source, mode)
}

/// Parse the given Python source code using the specified [`Mode`] and [`TextSize`].
///
/// This function allows to specify the location of the source code, other than
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
/// use ruff_python_parser::lexer::lex;
/// use ruff_python_parser::{Mode, parse_tokens};
///
/// let source = "1 + 2";
/// let tokens = lex(source, Mode::Expression);
/// let expr = parse_tokens(tokens.collect(), source, Mode::Expression);
/// assert!(expr.is_ok());
/// ```
pub fn parse_tokens(tokens: Vec<LexResult>, source: &str, mode: Mode) -> Result<Mod, ParseError> {
    let program = Program::parse_tokens(source, tokens, mode);
    if program.is_valid() {
        Ok(program.into_ast())
    } else {
        Err(program.into_errors().into_iter().next().unwrap())
    }
}

/// Tokens represents a vector of [`LexResult`].
///
/// This should only include tokens up to and including the first error. This struct is created
/// by the [`tokenize`] function.
#[derive(Debug, Clone)]
pub struct Tokens(Vec<LexResult>);

impl Tokens {
    /// Returns an iterator over the [`TokenKind`] and the range corresponding to the tokens.
    pub fn kinds(&self) -> TokenKindIter {
        TokenKindIter::new(&self.0)
    }

    /// Consumes the [`Tokens`], returning the underlying vector of [`LexResult`].
    pub fn into_inner(self) -> Vec<LexResult> {
        self.0
    }
}

impl Deref for Tokens {
    type Target = [LexResult];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An iterator over the [`TokenKind`] and the corresponding range.
///
/// This struct is created by the [`Tokens::kinds`] method.
#[derive(Clone, Default)]
pub struct TokenKindIter<'a> {
    inner: std::iter::Flatten<std::slice::Iter<'a, LexResult>>,
}

impl<'a> TokenKindIter<'a> {
    /// Create a new iterator from a slice of [`LexResult`].
    pub fn new(tokens: &'a [LexResult]) -> Self {
        Self {
            inner: tokens.iter().flatten(),
        }
    }

    /// Return the next value without advancing the iterator.
    pub fn peek(&mut self) -> Option<(TokenKind, TextRange)> {
        self.clone().next()
    }
}

impl Iterator for TokenKindIter<'_> {
    type Item = (TokenKind, TextRange);

    fn next(&mut self) -> Option<Self::Item> {
        let &(ref tok, range) = self.inner.next()?;
        Some((TokenKind::from_token(tok), range))
    }
}

impl FusedIterator for TokenKindIter<'_> {}

impl DoubleEndedIterator for TokenKindIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let &(ref tok, range) = self.inner.next_back()?;
        Some((TokenKind::from_token(tok), range))
    }
}

/// Collect tokens up to and including the first error.
pub fn tokenize(contents: &str, mode: Mode) -> Tokens {
    let mut tokens: Vec<LexResult> = allocate_tokens_vec(contents);
    for tok in lexer::lex(contents, mode) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }

    Tokens(tokens)
}

/// Tokenizes all tokens.
///
/// It differs from [`tokenize`] in that it tokenizes all tokens and doesn't stop
/// after the first `Err`.
pub fn tokenize_all(contents: &str, mode: Mode) -> Vec<LexResult> {
    let mut tokens = allocate_tokens_vec(contents);
    for token in lexer::lex(contents, mode) {
        tokens.push(token);
    }
    tokens
}

/// Allocates a [`Vec`] with an approximated capacity to fit all tokens
/// of `contents`.
///
/// See [#9546](https://github.com/astral-sh/ruff/pull/9546) for a more detailed explanation.
pub fn allocate_tokens_vec(contents: &str) -> Vec<LexResult> {
    Vec::with_capacity(approximate_tokens_lower_bound(contents))
}

/// Approximates the number of tokens when lexing `contents`.
fn approximate_tokens_lower_bound(contents: &str) -> usize {
    contents.len().saturating_mul(15) / 100
}

/// Parse a full Python program from its tokens.
pub fn parse_program_tokens(
    tokens: Tokens,
    source: &str,
    is_jupyter_notebook: bool,
) -> anyhow::Result<Suite, ParseError> {
    let mode = if is_jupyter_notebook {
        Mode::Ipython
    } else {
        Mode::Module
    };
    match parse_tokens(tokens.into_inner(), source, mode)? {
        Mod::Module(m) => Ok(m.body),
        Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
    }
}

/// Control in the different modes by which a source file can be parsed.
///
/// The mode argument specifies in what way code must be parsed.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Mode {
    /// The code consists of a sequence of statements.
    Module,

    /// The code consists of a single expression.
    Expression,

    /// The code consists of a sequence of statements which can include the
    /// escape commands that are part of IPython syntax.
    ///
    /// ## Supported escape commands:
    ///
    /// - [Magic command system] which is limited to [line magics] and can start
    ///   with `?` or `??`.
    /// - [Dynamic object information] which can start with `?` or `??`.
    /// - [System shell access] which can start with `!` or `!!`.
    /// - [Automatic parentheses and quotes] which can start with `/`, `;`, or `,`.
    ///
    /// [Magic command system]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#magic-command-system
    /// [line magics]: https://ipython.readthedocs.io/en/stable/interactive/magics.html#line-magics
    /// [Dynamic object information]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#dynamic-object-information
    /// [System shell access]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#system-shell-access
    /// [Automatic parentheses and quotes]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#automatic-parentheses-and-quotes
    Ipython,
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" | "single" => Ok(Mode::Module),
            "eval" => Ok(Mode::Expression),
            "ipython" => Ok(Mode::Ipython),
            _ => Err(ModeParseError),
        }
    }
}

/// A type that can be represented as [Mode].
pub trait AsMode {
    fn as_mode(&self) -> Mode;
}

impl AsMode for PySourceType {
    fn as_mode(&self) -> Mode {
        match self {
            PySourceType::Python | PySourceType::Stub => Mode::Module,
            PySourceType::Ipynb => Mode::Ipython,
        }
    }
}

/// Returned when a given mode is not valid.
#[derive(Debug)]
pub struct ModeParseError;

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode must be "exec", "eval", "ipython", or "single""#)
    }
}
