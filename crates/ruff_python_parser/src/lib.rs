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
//! these tokens are then consumed by the ruff_python_parser, which matches them against a set of
//! grammar rules to verify that the source code is syntactically valid and to construct
//! an AST that represents the source code.
//!
//! During parsing, the ruff_python_parser consumes the tokens generated by the lexer and constructs
//! a tree representation of the source code. The tree is made up of nodes that represent
//! the different syntactic constructs of the language. If the source code is syntactically
//! invalid, parsing fails and an error is returned. After a successful parse, the AST can
//! be used to perform further analysis on the source code. Continuing with the example
//! above, the AST generated by the ruff_python_parser would _roughly_ look something like this:
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
//! Note: The Tokens/ASTs shown above are not the exact tokens/ASTs generated by the ruff_python_parser.
//!
//! ## Source code layout:
//!
//! The functionality of this crate is split into several modules:
//!
//! - token: This module contains the definition of the tokens that are generated by the lexer.
//! - [lexer]: This module contains the lexer and is responsible for generating the tokens.
//! - ruff_python_parser: This module contains an interface to the ruff_python_parser and is responsible for generating the AST.
//!     - Functions and strings have special parsing requirements that are handled in additional files.
//! - mode: This module contains the definition of the different modes that the ruff_python_parser can be in.
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
//! These tokens can be directly fed into the ruff_python_parser to generate an AST:
//!
//! ```
//! use ruff_python_parser::{lexer::lex, Mode, parse_tokens};
//!
//! let python_source = r#"
//! def is_odd(i):
//!    return bool(i & 1)
//! "#;
//! let tokens = lex(python_source, Mode::Module);
//! let ast = parse_tokens(tokens, Mode::Module, "<embedded>");
//!
//! assert!(ast.is_ok());
//! ```
//!
//! Alternatively, you can use one of the other `parse_*` functions to parse a string directly without using a specific
//! mode or tokenizing the source beforehand:
//!
//! ```
//! use ruff_python_parser::{Parse};
//! use ruff_python_ast as ast;
//!
//! let python_source = r#"
//! def is_odd(i):
//!   return bool(i & 1)
//! "#;
//! let ast = ast::Suite::parse(python_source, "<embedded>");
//!
//! assert!(ast.is_ok());
//! ```
//!
//! [lexical analysis]: https://en.wikipedia.org/wiki/Lexical_analysis
//! [parsing]: https://en.wikipedia.org/wiki/Parsing
//! [lexer]: crate::lexer

use crate::lexer::LexResult;
pub use parse::Parse;
pub use parser::{parse, parse_starts_at, parse_tokens, ParseError, ParseErrorType};
#[allow(deprecated)]
pub use parser::{parse_expression, parse_expression_starts_at, parse_program};
use ruff_python_ast::{CmpOp, Expr, Mod, ModModule, Ranged, Suite};
use ruff_text_size::{TextRange, TextSize};
pub use string::FStringErrorType;
pub use token::{StringKind, Tok, TokenKind};

mod function;
// Skip flattening lexer to distinguish from full ruff_python_parser
mod context;
pub mod lexer;
mod parse;
mod parser;
mod soft_keywords;
mod string;
mod token;
pub mod typing;

/// Collect tokens up to and including the first error.
pub fn tokenize(contents: &str) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = vec![];
    for tok in lexer::lex(contents, Mode::Module) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }
    tokens
}

/// Parse a full Python program from its tokens.
pub fn parse_program_tokens(
    lxr: Vec<LexResult>,
    source_path: &str,
) -> anyhow::Result<Suite, ParseError> {
    parser::parse_tokens(lxr, Mode::Module, source_path).map(|top| match top {
        Mod::Module(ModModule { body, .. }) => body,
        _ => unreachable!(),
    })
}

/// Return the `Range` of the first `Tok::Colon` token in a `Range`.
pub fn first_colon_range(range: TextRange, source: &str) -> Option<TextRange> {
    let contents = &source[range];
    let range = lexer::lex_starts_at(contents, Mode::Module, range.start())
        .flatten()
        .find(|(tok, _)| tok.is_colon())
        .map(|(_, range)| range);
    range
}

/// Extract all [`CmpOp`] operators from an expression snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on [`CmpOp`] nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies [`CmpOp`] nodes, annotating them with valid ranges.
pub fn locate_cmp_ops(expr: &Expr, source: &str) -> Vec<LocatedCmpOp> {
    // If `Expr` is a multi-line expression, we need to parenthesize it to
    // ensure that it's lexed correctly.
    let contents = &source[expr.range()];
    let parenthesized_contents = format!("({contents})");
    let mut tok_iter = lexer::lex(&parenthesized_contents, Mode::Expression)
        .flatten()
        .skip(1)
        .map(|(tok, range)| (tok, range - TextSize::from(1)))
        .filter(|(tok, _)| !matches!(tok, Tok::NonLogicalNewline | Tok::Comment(_)))
        .peekable();

    let mut ops: Vec<LocatedCmpOp> = vec![];
    let mut count = 0u32;
    loop {
        let Some((tok, range)) = tok_iter.next() else {
            break;
        };
        if matches!(tok, Tok::Lpar) {
            count = count.saturating_add(1);
            continue;
        } else if matches!(tok, Tok::Rpar) {
            count = count.saturating_sub(1);
            continue;
        }
        if count == 0 {
            match tok {
                Tok::Not => {
                    if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::In))
                    {
                        ops.push(LocatedCmpOp::new(
                            TextRange::new(range.start(), next_range.end()),
                            CmpOp::NotIn,
                        ));
                    }
                }
                Tok::In => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::In));
                }
                Tok::Is => {
                    let op = if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::Not))
                    {
                        LocatedCmpOp::new(
                            TextRange::new(range.start(), next_range.end()),
                            CmpOp::IsNot,
                        )
                    } else {
                        LocatedCmpOp::new(range, CmpOp::Is)
                    };
                    ops.push(op);
                }
                Tok::NotEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::NotEq));
                }
                Tok::EqEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Eq));
                }
                Tok::GreaterEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::GtE));
                }
                Tok::Greater => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Gt));
                }
                Tok::LessEqual => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::LtE));
                }
                Tok::Less => {
                    ops.push(LocatedCmpOp::new(range, CmpOp::Lt));
                }
                _ => {}
            }
        }
    }
    ops
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedCmpOp {
    pub range: TextRange,
    pub op: CmpOp,
}

impl LocatedCmpOp {
    fn new<T: Into<TextRange>>(range: T, op: CmpOp) -> Self {
        Self {
            range: range.into(),
            op,
        }
    }
}

/// Control in the different modes by which a source file can be parsed.
/// The mode argument specifies in what way code must be parsed.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Mode {
    /// The code consists of a sequence of statements.
    Module,
    /// The code consists of a sequence of interactive statement.
    Interactive,
    /// The code consists of a single expression.
    Expression,
    /// The code consists of a sequence of statements which are part of a
    /// Jupyter Notebook and thus could include escape commands scoped to
    /// a single line.
    ///
    /// ## Limitations:
    ///
    /// For [Dynamic object information], the escape characters (`?`, `??`)
    /// must be used before an object. For example, `?foo` will be recognized,
    /// but `foo?` will not.
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
    Jupyter,
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" | "single" => Ok(Mode::Module),
            "eval" => Ok(Mode::Expression),
            "jupyter" => Ok(Mode::Jupyter),
            _ => Err(ModeParseError),
        }
    }
}

/// Returned when a given mode is not valid.
#[derive(Debug)]
pub struct ModeParseError;

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode must be "exec", "eval", "jupyter", or "single""#)
    }
}

#[rustfmt::skip]
mod python {
    #![allow(unreachable_pub)]

    #[cfg(feature = "lalrpop")]
    include!(concat!(env!("OUT_DIR"), "/src/python.rs"));

    #[cfg(not(feature = "lalrpop"))]
    include!("python.rs");
}

#[cfg(test)]
mod tests {
    use crate::Parse;
    use crate::{first_colon_range, locate_cmp_ops, LocatedCmpOp};
    use anyhow::Result;
    use ruff_python_ast::CmpOp;
    use ruff_python_ast::Expr;
    use ruff_text_size::{TextLen, TextRange, TextSize};

    #[test]
    fn extract_first_colon_range() {
        let contents = "with a: pass";
        let range = first_colon_range(
            TextRange::new(TextSize::from(0), contents.text_len()),
            contents,
        )
        .unwrap();
        assert_eq!(&contents[range], ":");
        assert_eq!(range, TextRange::new(TextSize::from(6), TextSize::from(7)));
    }

    #[test]
    fn extract_cmp_op_location() -> Result<()> {
        let contents = "x == 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Eq
            )]
        );

        let contents = "x != 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        let contents = "x is 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Is
            )]
        );

        let contents = "x is not 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::IsNot
            )]
        );

        let contents = "x in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::In
            )]
        );

        let contents = "x not in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::NotIn
            )]
        );

        let contents = "x != (1 is not 2)";
        let expr = Expr::parse(contents, "<filename>")?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        Ok(())
    }
}
