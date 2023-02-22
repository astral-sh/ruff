//! Contains the interface to the Python parser.
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

use crate::{
    ast::{self, Location},
    lexer::{self, LexResult, LexicalError, LexicalErrorType},
    mode::Mode,
    python,
    token::Tok,
};
use itertools::Itertools;
use std::iter;

pub(super) use lalrpop_util::ParseError as LalrpopError;

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
/// use rustpython_parser as parser;
/// let source = r#"
/// def foo():
///    return 42
///
/// print(foo())
/// "#;
/// let program = parser::parse_program(source, "<embedded>");
/// assert!(program.is_ok());
/// ```
pub fn parse_program(source: &str, source_path: &str) -> Result<ast::Suite, ParseError> {
    parse(source, Mode::Module, source_path).map(|top| match top {
        ast::Mod::Module { body, .. } => body,
        _ => unreachable!(),
    })
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
/// extern crate num_bigint;
/// use rustpython_parser as parser;
/// let expr = parser::parse_expression("1 + 2", "<embedded>");
///
/// assert!(expr.is_ok());
///
/// ```
pub fn parse_expression(source: &str, path: &str) -> Result<ast::Expr, ParseError> {
    parse_expression_located(source, path, Location::new(1, 0))
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
/// use rustpython_parser::{ast::Location, parse_expression_located};
///
/// let expr = parse_expression_located("1 + 2", "<embedded>", Location::new(5, 20));
/// assert!(expr.is_ok());
/// ```
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

/// Parse the given Python source code using the specified [`Mode`].
///
/// This function is the most general function to parse Python code. Based on the [`Mode`] supplied,
/// it can be used to parse a single expression, a full Python program or an interactive expression.
///
/// # Example
///
/// If we want to parse a simple expression, we can use the [`Mode::Expression`] mode during
/// parsing:
///
/// ```
/// use rustpython_parser::{Mode, parse};
///
/// let expr = parse("1 + 2", Mode::Expression, "<embedded>");
/// assert!(expr.is_ok());
/// ```
///
/// Alternatively, we can parse a full Python program consisting of multiple lines:
///
/// ```
/// use rustpython_parser::{Mode, parse};
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
pub fn parse(source: &str, mode: Mode, source_path: &str) -> Result<ast::Mod, ParseError> {
    parse_located(source, mode, source_path, Location::new(1, 0))
}

/// Parse the given Python source code using the specified [`Mode`] and [`Location`].
///
/// This function allows to specify the location of the the source code, other than
/// that, it behaves exactly like [`parse`].
///
/// # Example
///
/// ```
/// use rustpython_parser::{ast::Location, Mode, parse_located};
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
/// let program = parse_located(source, Mode::Module, "<embedded>", Location::new(1, 0));
/// assert!(program.is_ok());
/// ```
pub fn parse_located(
    source: &str,
    mode: Mode,
    source_path: &str,
    location: Location,
) -> Result<ast::Mod, ParseError> {
    let lxr = lexer::lex_located(source, mode, location);
    parse_tokens(lxr, mode, source_path)
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
/// use rustpython_parser::{lexer::lex, Mode, parse_tokens};
///
/// let expr = parse_tokens(lex("1 + 2", Mode::Expression), Mode::Expression, "<embedded>");
/// assert!(expr.is_ok());
/// ```
pub fn parse_tokens(
    lxr: impl IntoIterator<Item = LexResult>,
    mode: Mode,
    source_path: &str,
) -> Result<ast::Mod, ParseError> {
    let marker_token = (Default::default(), mode.to_marker(), Default::default());
    let lexer = iter::once(Ok(marker_token))
        .chain(lxr)
        .filter_ok(|(_, tok, _)| !matches!(tok, Tok::Comment { .. } | Tok::NonLogicalNewline));
    python::TopParser::new()
        .parse(lexer.into_iter())
        .map_err(|e| parse_error_from_lalrpop(e, source_path))
}

/// Represents represent errors that occur during parsing and are
/// returned by the `parse_*` functions.
pub type ParseError = rustpython_compiler_core::BaseError<ParseErrorType>;

/// Represents the different types of errors that can occur during parsing.
#[derive(Debug, PartialEq, thiserror::Error)]
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

// Convert `lalrpop_util::ParseError` to our internal type
fn parse_error_from_lalrpop(
    err: LalrpopError<Location, Tok, LexicalError>,
    source_path: &str,
) -> ParseError {
    let source_path = source_path.to_owned();
    match err {
        // TODO: Are there cases where this isn't an EOF?
        LalrpopError::InvalidToken { location } => ParseError {
            error: ParseErrorType::Eof,
            location,
            source_path,
        },
        LalrpopError::ExtraToken { token } => ParseError {
            error: ParseErrorType::ExtraToken(token.1),
            location: token.0,
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
                location: token.0.with_col_offset(1),
                source_path,
            }
        }
        LalrpopError::UnrecognizedEOF { location, expected } => {
            // This could be an initial indentation error that we should ignore
            let indent_error = expected == ["Indent"];
            if indent_error {
                ParseError {
                    error: ParseErrorType::Lexical(LexicalErrorType::IndentationError),
                    location,
                    source_path,
                }
            } else {
                ParseError {
                    error: ParseErrorType::Eof,
                    location,
                    source_path,
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
            ParseErrorType::Lexical(LexicalErrorType::TabError)
                | ParseErrorType::Lexical(LexicalErrorType::TabsAfterSpaces)
        )
    }
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
    fn test_star_index() {
        let source = "\
array_slice = array[0, *idxs, -1]
array[0, *idxs, -1] = array_slice
array[*idxs_to_select, *idxs_to_select]
array[3:5, *idxs_to_select]
";
        let parse_ast = parse_program(source, "<test>").unwrap();
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
        let parse_ast = parse_expression(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try() {
        let parse_ast = parse_program(
            r#"try:
    raise ValueError(1)
except TypeError as e:
    print(f'caught {type(e)}')
except OSError as e:
    print(f'caught {type(e)}')"#,
            "<test>",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_try_star() {
        let parse_ast = parse_program(
            r#"try:
    raise ExceptionGroup("eg",
        [ValueError(1), TypeError(2), OSError(3), OSError(4)])
except* TypeError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')
except* OSError as e:
    print(f'caught {type(e)} with nested {e.exceptions}')"#,
            "<test>",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_dict_unpacking() {
        let parse_ast = parse_expression(r#"{"a": "b", **c, "d": "e"}"#, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_modes() {
        let source = "a[0][1][2][3][4]";

        assert!(parse(&source, Mode::Expression, "<embedded>").is_ok());
        assert!(parse(&source, Mode::Module, "<embedded>").is_ok());
        assert!(parse(&source, Mode::Interactive, "<embedded>").is_ok());
    }

    #[test]
    fn test_match_as_identifier() {
        let parse_ast = parse_program(
            r#"
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
"#,
            "<test>",
        )
        .unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_match_complex() {
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
        let parse_ast = parse_program(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
