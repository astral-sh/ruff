//! Python parsing.
//!
//! Use this module to parse python code into an AST.
//! There are three ways to parse python code. You could
//! parse a whole program, a single statement, or a single
//! expression.

use crate::lexer::{LexResult, Tok};
pub use crate::mode::Mode;
use crate::{ast, error::ParseError, lexer, python};
use ast::Location;
use itertools::Itertools;
use std::iter;

/*
 * Parse python code.
 * Grammar may be inspired by antlr grammar for python:
 * https://github.com/antlr/grammars-v4/tree/master/python3
 */

/// Parse a full python program, containing usually multiple lines.
pub fn parse_program(source: &str, source_path: &str) -> Result<ast::Suite, ParseError> {
    parse(source, Mode::Module, source_path).map(|top| match top {
        ast::Mod::Module { body, .. } => body,
        _ => unreachable!(),
    })
}

/// Parses a python expression
///
/// # Example
/// ```
/// extern crate num_bigint;
/// use rustpython_parser::{parser, ast};
/// let expr = parser::parse_expression("1 + 2", "<embedded>").unwrap();
///
/// assert_eq!(
///     expr,
///     ast::Expr {
///         location: ast::Location::new(1, 0),
///         end_location: Some(ast::Location::new(1, 5)),
///         custom: (),
///         node: ast::ExprKind::BinOp {
///             left: Box::new(ast::Expr {
///                 location: ast::Location::new(1, 0),
///                 end_location: Some(ast::Location::new(1, 1)),
///                 custom: (),
///                 node: ast::ExprKind::Constant {
///                     value: ast::Constant::Int(1.into()),
///                     kind: None,
///                 }
///             }),
///             op: ast::Operator::Add,
///             right: Box::new(ast::Expr {
///                 location: ast::Location::new(1, 4),
///                 end_location: Some(ast::Location::new(1, 5)),
///                 custom: (),
///                 node: ast::ExprKind::Constant {
///                     value: ast::Constant::Int(2.into()),
///                     kind: None,
///                 }
///             })
///         }
///     },
/// );
///
/// ```
pub fn parse_expression(source: &str, path: &str) -> Result<ast::Expr, ParseError> {
    parse_expression_located(source, path, Location::new(1, 0))
}

pub fn parse_expression_located(
    source: &str,
    path: &str,
    location: Location,
) -> Result<ast::Expr, ParseError> {
    parse_located(source, Mode::Expression, path, location).map(|top| match top {
        ast::Mod::Expression { body } => *body,
        _ => unreachable!(),
    })
}

// Parse a given source code
pub fn parse(source: &str, mode: Mode, source_path: &str) -> Result<ast::Mod, ParseError> {
    parse_located(source, mode, source_path, Location::new(1, 0))
}

// Parse a given source code from a given location
pub fn parse_located(
    source: &str,
    mode: Mode,
    source_path: &str,
    location: Location,
) -> Result<ast::Mod, ParseError> {
    let lxr = lexer::make_tokenizer_located(source, location);
    parse_tokens(lxr, mode, source_path)
}

// Parse a given token iterator.
pub fn parse_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    mode: Mode,
    source_path: &str,
) -> Result<ast::Mod, ParseError> {
    let marker_token = (Default::default(), mode.to_marker(), Default::default());
    let tokenizer = iter::once(Ok(marker_token))
        .chain(lxr)
        .filter_ok(|(_, tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline));

    python::TopParser::new()
        .parse(tokenizer)
        .map_err(|e| crate::error::parse_error_from_lalrpop(e, source_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let parse_ast = parse_program("", "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string() {
        let source = "'Hello world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string() {
        let source = "f'Hello world'";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_hello() {
        let source = "print('Hello world')";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_print_2() {
        let source = "print('Hello world', 2)";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_kwargs() {
        let source = "my_func('positional', keyword=2)";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_elif_else() {
        let source = "if 1: 10\nelif 2: 20\nelse: 30";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_lambda() {
        let source = "lambda x, y: x * y"; // lambda(x, y): x * y";
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_tuples() {
        let source = "a, b = 4, 5";

        insta::assert_debug_snapshot!(parse_program(source, "<test>").unwrap());
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
        insta::assert_debug_snapshot!(parse_program(source, "<test>").unwrap());
    }

    #[test]
    fn test_parse_dict_comprehension() {
        let source = "{x1: x2 for y in z}";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_list_comprehension() {
        let source = "[x for y in z]";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_double_list_comprehension() {
        let source = "[x for y, y2 in z for a in b if a < 5 if a > 10]";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_generator_comprehension() {
        let source = "(x for y in z)";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_named_expression_generator_comprehension() {
        let source = "(x := y + 1 for y in z)";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_if_else_generator_comprehension() {
        let source = "(x if y else y for y in z)";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_boolop_or() {
        let source = "x or y";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_boolop_and() {
        let source = "x and y";
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_slice() {
        let source = "x[1:2:3]";
        let parse_ast = parse_expression(source, "<test>").unwrap();
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
        insta::assert_debug_snapshot!(parse_program(source, "<test>").unwrap());
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
            assert!(parse_program(source, "<test>").is_err());
        }
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
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_dict_unpacking() {
        let parse_ast = parse_expression(r#"{"a": "b", **c, "d": "e"}"#, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
