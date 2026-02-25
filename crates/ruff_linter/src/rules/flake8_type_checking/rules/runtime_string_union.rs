use crate::Violation;
use crate::checkers::ast::Checker;
use crate::{Fix, FixAvailability};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast};
use ruff_python_ast::{Expr, Operator};
use ruff_python_parser::semantic_errors::SemanticSyntaxContext;
use ruff_text_size::Ranged;

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
/// var: "Foo" | None
///
///
/// class Foo: ...
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// var: Foo | None
///
///
/// class Foo: ...
/// ```
///
/// Or, extend the quotes to include the entire union:
/// ```python
/// var: "Foo | None"
///
///
/// class Foo: ...
/// ```
///
/// ## References
/// - [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0")]
pub(crate) struct RuntimeStringUnion;

impl Violation for RuntimeStringUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid string member in `X | Y`-style union type".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Add `from __future__ import annotations`".to_string())
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

    for string in &strings {
        let mut diagnostic = checker.report_diagnostic(RuntimeStringUnion, string.range());
        if checker.settings().future_annotations && !checker.future_annotations_or_stub() {
            diagnostic.set_fix(Fix::unsafe_edit(checker.importer().add_future_import()));
        }
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
