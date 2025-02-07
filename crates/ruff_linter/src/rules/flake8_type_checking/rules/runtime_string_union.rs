use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of string literals in `X | Y`-style union types.
///
/// ## Why is this bad?
/// [PEP 604] introduced a new syntax for union type annotations based on the
/// `|` operator.
///
/// While Python's type annotations can typically be wrapped in strings to
/// avoid runtime evaluation, the use of a string member within an `X | Y`-style
/// union type will cause a runtime error.
///
/// Instead, remove the quotes, wrap the _entire_ union in quotes, or use
/// `from __future__ import annotations` to disable runtime evaluation of
/// annotations entirely.
///
/// ## Example
/// ```python
/// var: str | "int"
/// ```
///
/// Use instead:
/// ```python
/// var: str | int
/// ```
///
/// Or, extend the quotes to include the entire union:
/// ```python
/// var: "str | int"
/// ```
///
/// ## References
/// - [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[derive(ViolationMetadata)]
pub(crate) struct RuntimeStringUnion;

impl Violation for RuntimeStringUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid string member in `X | Y`-style union type".to_string()
    }
}

/// TC010
pub(crate) fn runtime_string_union(checker: &Checker, expr: &Expr) {
    if !checker.semantic().in_type_definition() {
        return;
    }

    if !checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Search for strings within the binary operator.
    let mut strings = Vec::new();
    traverse_op(expr, &mut strings);

    for string in strings {
        checker.report_diagnostic(Diagnostic::new(RuntimeStringUnion, string.range()));
    }
}

/// Collect all string members in possibly-nested binary `|` expressions.
fn traverse_op<'a>(expr: &'a Expr, strings: &mut Vec<&'a Expr>) {
    match expr {
        Expr::StringLiteral(_) => {
            strings.push(expr);
        }
        Expr::BytesLiteral(_) => {
            strings.push(expr);
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            right,
            op: Operator::BitOr,
            ..
        }) => {
            traverse_op(left, strings);
            traverse_op(right, strings);
        }
        _ => {}
    }
}
