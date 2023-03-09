use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

/// W1514: Using open without explicitly specifying an encoding
/// (unspecified-encoding)
///
/// ## What it does
/// Checks for a valid encoding field when opening files.
///
/// ## Why is this bad?
/// It is better to specify an encoding when opening documents.
/// Using the system default implicitly can create problems on other operating systems.
/// See https://peps.python.org/pep-0597/
///
/// ## Example
/// ```python
/// def foo(file_path):
///     with open(file_path) as file:  # [unspecified-encoding]
///       contents = file.read()
/// ```
///
/// Use instead:
/// ```python
/// def foo(file_path):
///     with open(file_path, encoding="utf-8") as file:
///         contents = file.read()
/// ```
#[violation]
pub struct UnspecifiedEncoding;

impl Violation for UnspecifiedEncoding {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using open without explicitly specifying an encoding")
    }
}

const OPEN_FUNC_NAME: &str = "open";
const ENCODING_MODE_ARGUMENT: &str = "mode";
const ENCODING_KEYWORD_ARGUMENT: &str = "encoding";

/// W1514
pub fn unspecified_encoding(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    // If `open` has been rebound, skip this check entirely.
    if !checker.ctx.is_builtin(OPEN_FUNC_NAME) {
        return;
    }
    // Look for open().
    if !matches!(&func.node, ExprKind::Name {id, ..} if id == OPEN_FUNC_NAME) {
        return;
    }

    // If the mode arg has "b", skip this check.
    let call_args = SimpleCallArgs::new(args, keywords);
    let mode_arg = call_args.get_argument(ENCODING_MODE_ARGUMENT, Some(1));
    if let Some(mode) = mode_arg {
        if let ExprKind::Constant {
            value: Constant::Str(mode_param_value),
            ..
        } = &mode.node
        {
            if mode_param_value.as_str().contains('b') {
                return;
            }
        }
    }

    // Check encoding for missing or None values.
    let encoding_arg = call_args.get_argument(ENCODING_KEYWORD_ARGUMENT, Some(3));
    if let Some(keyword) = encoding_arg {
        // encoding=None
        if let ExprKind::Constant {
            value: Constant::None,
            ..
        } = &keyword.node
        {
            checker
                .diagnostics
                .push(Diagnostic::new(UnspecifiedEncoding, Range::from(func)));
        }
    } else {
        // Encoding not found
        checker
            .diagnostics
            .push(Diagnostic::new(UnspecifiedEncoding, Range::from(func)));
    }
}
