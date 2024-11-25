use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprBinOp, ExprNoneLiteral, Operator};
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use smallvec::SmallVec;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `Literal[None]` annotations.
///
/// ## Why is this bad?
/// While `Literal[None]` is a valid type annotation, it is semantically equivalent to `None`.
/// Prefer `None` over `Literal[None]` for both consistency and readability.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// Literal[None]
/// Literal[1, 2, 3, "foo", 5, None]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// None
/// Literal[1, 2, 3, "foo", 5] | None
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe unless the literal contains comments.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
#[violation]
pub struct RedundantNoneLiteral {
    other_literal_elements_seen: bool,
}

impl Violation for RedundantNoneLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.other_literal_elements_seen {
            "`Literal[None, ...]` can be replaced with `Literal[...] | None`".to_string()
        } else {
            "`Literal[None]` can be replaced with `None`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(if self.other_literal_elements_seen {
            "Replace with `Literal[...] | None`".to_string()
        } else {
            "Replace with `None`".to_string()
        })
    }
}

/// RUF037
pub(crate) fn redundant_none_literal<'a>(checker: &mut Checker, literal_expr: &'a Expr) {
    if !checker.semantic().seen_typing() {
        return;
    }

    let mut none_exprs: SmallVec<[&ExprNoneLiteral; 1]> = SmallVec::new();
    let mut other_literal_elements_seen = false;

    let mut find_none = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::NoneLiteral(none_expr) = expr {
            none_exprs.push(none_expr);
        } else {
            other_literal_elements_seen = true;
        }
    };

    traverse_literal(&mut find_none, checker.semantic(), literal_expr);

    if none_exprs.is_empty() {
        return;
    }

    // Provide a [`Fix`] when the complete `Literal` can be replaced. Applying the fix
    // can leave an unused import to be fixed by the `unused-import` rule.
    let fix = if other_literal_elements_seen {
        None
    } else {
        // Avoid producing code that would raise an exception when
        // `Literal[None] | None` would be fixed to `None | None`.
        // Instead fix to `None`. No action needed for `typing.Union`,
        // as `Union[None, None]` is valid Python.
        // See https://github.com/astral-sh/ruff/issues/14567.
        let replacement_range = if let Some(Expr::BinOp(ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
            range: parent_range,
        })) = checker.semantic().current_expression_parent()
        {
            if matches!(**left, Expr::NoneLiteral(_)) || matches!(**right, Expr::NoneLiteral(_)) {
                *parent_range
            } else {
                literal_expr.range()
            }
        } else {
            literal_expr.range()
        };

        Some(Fix::applicable_edit(
            Edit::range_replacement("None".to_string(), replacement_range),
            if checker.comment_ranges().intersects(literal_expr.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ))
    };

    for none_expr in none_exprs {
        let mut diagnostic = Diagnostic::new(
            RedundantNoneLiteral {
                other_literal_elements_seen,
            },
            none_expr.range(),
        );
        if let Some(ref fix) = fix {
            diagnostic.set_fix(fix.clone());
        }
        checker.diagnostics.push(diagnostic);
    }
}
