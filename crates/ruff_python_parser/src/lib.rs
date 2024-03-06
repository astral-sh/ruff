//! This crate can be used to parse Python source code into an Abstract
//! Syntax Tree.
//!
//! ## Overview:
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
//! these tokens are then consumed by the `ruff_python_parser`, which matches them against a set of
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
//! Note: The Tokens/ASTs shown above are not the exact tokens/ASTs generated by the `ruff_python_parser`.
//!
//! ## Source code layout:
//!
//! The functionality of this crate is split into several modules:
//!
//! - token: This module contains the definition of the tokens that are generated by the lexer.
//! - [lexer]: This module contains the lexer and is responsible for generating the tokens.
//! - `ruff_python_parser`: This module contains an interface to the `ruff_python_parser` and is responsible for generating the AST.
//!     - Functions and strings have special parsing requirements that are handled in additional files.
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
//! use ruff_python_parser::{Mode, parse_tokens, tokenize_all};
//!
//! let python_source = r#"
//! def is_odd(i):
//!    return bool(i & 1)
//! "#;
//! let tokens = tokenize_all(python_source, Mode::Module);
//! let ast = parse_tokens(tokens, python_source, Mode::Module);
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

pub use parser::{
    parse, parse_expression, parse_expression_starts_at, parse_program, parse_starts_at,
    parse_suite, parse_tokens, ParseError, ParseErrorType,
};
use ruff_python_ast::{Mod, PySourceType, Suite};
pub use string::FStringErrorType;
pub use token::{Tok, TokenKind};

use crate::lexer::LexResult;

mod context;
mod function;
mod invalid;
// Skip flattening lexer to distinguish from full ruff_python_parser
pub mod lexer;
mod parser;
mod soft_keywords;
mod string;
mod string_token_flags;
mod token;
mod token_source;
pub mod typing;

/// Collect tokens up to and including the first error.
pub fn tokenize(contents: &str, mode: Mode) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = allocate_tokens_vec(contents);
    for tok in lexer::lex(contents, mode) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }

    tokens
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
    tokens: Vec<LexResult>,
    source: &str,
    is_jupyter_notebook: bool,
) -> anyhow::Result<Suite, ParseError> {
    let mode = if is_jupyter_notebook {
        Mode::Ipython
    } else {
        Mode::Module
    };
    match parse_tokens(tokens, source, mode)? {
        Mod::Module(m) => Ok(m.body),
        Mod::Expression(_) => unreachable!("Mode::Module doesn't return other variant"),
    }
}

/// Control in the different modes by which a source file can be parsed.
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

#[rustfmt::skip]
#[allow(unreachable_pub)]
#[allow(clippy::type_complexity)]
#[allow(clippy::extra_unused_lifetimes)]
#[allow(clippy::needless_lifetimes)]
#[allow(clippy::unused_self)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::default_trait_access)]
#[allow(clippy::let_unit_value)]
#[allow(clippy::just_underscores_and_digits)]
#[allow(clippy::no_effect_underscore_binding)]
#[allow(clippy::trivially_copy_pass_by_ref)]
#[allow(clippy::option_option)]
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::uninlined_format_args)]
#[allow(clippy::cloned_instead_of_copied)]
mod python {

    #[cfg(feature = "lalrpop")]
    include!(concat!(env!("OUT_DIR"), "/src/python.rs"));

    #[cfg(not(feature = "lalrpop"))]
    include!("python.rs");
}
