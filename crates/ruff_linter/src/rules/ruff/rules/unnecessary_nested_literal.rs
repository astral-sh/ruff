use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{AnyNodeRef, Expr, ExprContext, ExprSubscript, ExprTuple};
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary nested `Literal`.
///
/// ## Why is this bad?
/// Prefer using a single `Literal`, which is equivalent and more concise.
///
/// Parameterization of literals by other literals is supported as an ergonomic
/// feature as proposed in [PEP 586], to enable patterns such as:
/// ```python
/// ReadOnlyMode         = Literal["r", "r+"]
/// WriteAndTruncateMode = Literal["w", "w+", "wt", "w+t"]
/// WriteNoTruncateMode  = Literal["r+", "r+t"]
/// AppendMode           = Literal["a", "a+", "at", "a+t"]
///
/// AllModes = Literal[ReadOnlyMode, WriteAndTruncateMode,
///                   WriteNoTruncateMode, AppendMode]
/// ```
///
/// As a consequence, type checkers also support nesting of literals
/// which is less readable than a flat `Literal`:
/// ```python
/// AllModes = Literal[Literal["r", "r+"], Literal["w", "w+", "wt", "w+t"],
///                   Literal["r+", "r+t"], Literal["a", "a+", "at", "a+t"]]
/// ```
///
/// ## Example
/// ```python
/// AllModes = Literal[
///     Literal["r", "r+"],
///     Literal["w", "w+", "wt", "w+t"],
///     Literal["r+", "r+t"],
///     Literal["a", "a+", "at", "a+t"],
/// ]
/// ```
///
/// Use instead:
/// ```python
/// AllModes = Literal[
///     "r", "r+", "w", "w+", "wt", "w+t", "r+", "r+t", "a", "a+", "at", "a+t"
/// ]
/// ```
///
/// or assign the literal to a variable as in the first example.
///
/// ## Fix safety
/// The fix for this rule is marked as unsafe when the `Literal` slice is split
/// across multiple lines and some of the lines have trailing comments.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
///
/// [PEP 586](https://peps.python.org/pep-0586/)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryNestedLiteral;

impl Violation for UnnecessaryNestedLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary nested `Literal`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with flattened `Literal`".to_string())
    }
}

/// RUF039
pub(crate) fn unnecessary_nested_literal<'a>(checker: &Checker, literal_expr: &'a Expr) {
    let mut is_nested = false;

    // Traverse the type expressions in the `Literal`.
    traverse_literal(
        &mut |_: &'a Expr, parent: &'a Expr| {
            // If the parent is not equal to the `literal_expr` then we know we are traversing recursively.
            if !AnyNodeRef::ptr_eq(parent.into(), literal_expr.into()) {
                is_nested = true;
            }
        },
        checker.semantic(),
        literal_expr,
    );

    if !is_nested {
        return;
    }

    // Collect the literal nodes for the fix
    let mut nodes: Vec<&Expr> = Vec::new();

    traverse_literal(
        &mut |expr, _| {
            nodes.push(expr);
        },
        checker.semantic(),
        literal_expr,
    );

    let mut diagnostic = Diagnostic::new(UnnecessaryNestedLiteral, literal_expr.range());

    // Create a [`Fix`] that flattens all nodes.
    if let Expr::Subscript(subscript) = literal_expr {
        let subscript = Expr::Subscript(ExprSubscript {
            slice: Box::new(if let [elt] = nodes.as_slice() {
                (*elt).clone()
            } else {
                Expr::Tuple(ExprTuple {
                    elts: nodes.into_iter().cloned().collect(),
                    range: TextRange::default(),
                    ctx: ExprContext::Load,
                    parenthesized: false,
                })
            }),
            value: subscript.value.clone(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
        });
        let fix = Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(&subscript), literal_expr.range()),
            if checker.comment_ranges().intersects(literal_expr.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        );
        diagnostic.set_fix(fix);
    }

    checker.report_diagnostic(diagnostic);
}
