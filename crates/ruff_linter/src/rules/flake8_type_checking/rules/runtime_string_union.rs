use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_type_checking::helpers::{quote_type_expression, quotes_are_unremovable};

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
/// ## Preview
/// In preview mode this rule has a fix available. However the logic for the rule
/// has changed slightly as well. So there is a chance it will behave differently
/// in untested corner cases.
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
            ..
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
pub(crate) fn runtime_string_union(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().in_type_definition() {
        return;
    }

    // The union is only problematic at runtime. Even though stub files are never
    // executed, some of the nodes still end up having a runtime execution context
    if checker.source_type.is_stub() || !checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Search for strings within the binary operator.
    let mut strings = Vec::new();
    traverse_op(expr, &mut strings);

    for string in strings {
        checker.diagnostics.push(Diagnostic::new(
            RuntimeStringUnion { strategy: None },
            string.range(),
        ));
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

/// TC010 (preview version with fix)
pub(crate) fn runtime_string_union_preview(
    checker: &mut Checker,
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

    if quotes_are_unremovable(checker.semantic(), expr, checker.settings) {
        // extend the expression to the smallest possible expression that
        // can still be quoted safely
        let mut extended_expr = None;
        for node_id in checker.semantic().current_expression_ids() {
            let expr = checker
                .semantic()
                .expression(node_id)
                .expect("Expected expression");
            match checker.semantic().parent_expression(node_id) {
                Some(Expr::Subscript(parent)) => {
                    if expr == parent.value.as_ref() {
                        continue;
                    }
                }
                Some(Expr::Attribute(parent)) => {
                    if expr == parent.value.as_ref() {
                        continue;
                    }
                }
                Some(Expr::Call(parent)) => {
                    if expr == parent.func.as_ref() {
                        continue;
                    }
                }
                Some(Expr::BinOp(ast::ExprBinOp {
                    op: Operator::BitOr,
                    ..
                })) => {
                    continue;
                }
                _ => {}
            }

            extended_expr = Some(expr);
            break;
        }
        if let Some(extended_expr) = extended_expr {
            let edit = quote_type_expression(
                extended_expr,
                checker.semantic(),
                checker.stylist(),
                checker.locator(),
            );
            let fix = if checker.comment_ranges().intersects(extended_expr.range()) {
                Fix::unsafe_edit(edit)
            } else {
                Fix::safe_edit(edit)
            };
            let mut diagnostic = Diagnostic::new(
                RuntimeStringUnion {
                    strategy: Some(Strategy::ExtendQuotes),
                },
                annotation_expr.range(),
            );
            diagnostic.set_parent(extended_expr.range().start());
            diagnostic.set_fix(fix);
            checker.diagnostics.push(diagnostic);
        } else {
            // this is not fixable
            checker.diagnostics.push(Diagnostic::new(
                RuntimeStringUnion { strategy: None },
                annotation_expr.range(),
            ));
        }
        return;
    }

    // simply remove the quotes
    let mut diagnostic = Diagnostic::new(
        RuntimeStringUnion {
            strategy: Some(Strategy::RemoveQuotes),
        },
        annotation_expr.range(),
    );
    let edit = Edit::range_replacement(annotation_expr.value.to_string(), annotation_expr.range());
    if checker.comment_ranges().intersects(annotation_expr.range()) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
    checker.diagnostics.push(diagnostic);
}

/// Get the parent expression

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// The quotes should be removed.
    RemoveQuotes,
    /// The quotes should be extended to cover the entire union.
    ExtendQuotes,
}
