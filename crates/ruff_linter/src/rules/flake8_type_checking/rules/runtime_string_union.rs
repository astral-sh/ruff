use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_type_checking::helpers::{
    quote_annotation, quotes_are_unremovable, unquote_annotation_string,
};
use crate::{Fix, FixAvailability, Violation};

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
/// ## Preview
/// In preview mode this rule has a fix available.
///
/// ## Fix safety
/// This fix is safe as long as the fix doesn't remove a comment, which can happen
/// when the union spans multiple lines.
///
/// ## References
/// - [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0", category = Category::Correctness)]
pub(crate) struct RuntimeStringUnion {
    strategy: Option<Strategy>,
}

impl Violation for RuntimeStringUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid string member in `X | Y`-style union type".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let Self {
            strategy: Some(strategy),
        } = self
        else {
            return None;
        };
        match strategy {
            Strategy::RemoveQuotes => Some("Remove quotes".to_string()),
            Strategy::ExtendQuotes => Some("Extend quotes".to_string()),
        }
    }
}

/// TC010
pub(crate) fn runtime_string_union(
    checker: &Checker,
    expr: &Expr,
    annotation_expr: &ast::ExprStringLiteral,
) {
    // The union is only problematic at runtime. Even though stub files are never
    // executed, some of the nodes still end up having a runtime execution context
    if checker.source_type.is_stub() || !checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Are we actually part of a union?
    let Some(Expr::BinOp(ast::ExprBinOp {
        op: Operator::BitOr,
        ..
    })) = checker.semantic().current_expression_parent()
    else {
        return;
    };

    if quotes_are_unremovable(checker.semantic(), expr, checker.target_version()) {
        // extend the quotes
        let mut diagnostic = checker.report_diagnostic(
            RuntimeStringUnion {
                strategy: Some(Strategy::ExtendQuotes),
            },
            annotation_expr.range(),
        );
        // the edit tells us where our extended expression starts
        let edit = quote_annotation(
            checker
                .semantic()
                .current_expression_id()
                .expect("Expected to be called during expression traversal"),
            checker.semantic(),
            checker.stylist(),
            checker.locator(),
            checker.default_string_flags(),
        );
        diagnostic.set_parent(edit.range().start());
        if checker.settings().preview.is_enabled() {
            let fix = if checker.comment_ranges().intersects(edit.range()) {
                Fix::unsafe_edit(edit)
            } else {
                Fix::safe_edit(edit)
            };
            diagnostic.set_fix(fix);
        }
        return;
    }

    // simply remove the quotes
    let mut diagnostic = checker.report_diagnostic(
        RuntimeStringUnion {
            strategy: Some(Strategy::RemoveQuotes),
        },
        annotation_expr.range(),
    );
    if checker.settings().preview.is_enabled() {
        let edit = unquote_annotation_string(expr, annotation_expr, checker);
        if checker.comment_ranges().intersects(edit.range()) {
            diagnostic.set_fix(Fix::unsafe_edit(edit));
        } else {
            diagnostic.set_fix(Fix::safe_edit(edit));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// The quotes should be removed.
    RemoveQuotes,
    /// The quotes should be extended to cover the entire union.
    ExtendQuotes,
}
