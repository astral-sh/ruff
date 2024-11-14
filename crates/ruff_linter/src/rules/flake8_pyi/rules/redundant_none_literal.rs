use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `Literal[None]` annotations.
///
/// ## Why is this bad?
/// The `Literal[None]` annotation is equivalent to `None`, as the type `None` consists
/// of just a single value.
///
/// ## Example
/// ```python
/// Literal[None]
/// Literal[1, 2, 3, "foo", 5, None]
/// ```
///
/// Use instead:
/// ```python
/// None
/// Literal[1, 2, 3, "foo", 5] | None
/// ```
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
#[violation]
pub struct RedundantNoneLiteral {
    seen_others: bool,
}

impl Violation for RedundantNoneLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.seen_others {
            "`Literal[None, ...]` can be replaced with `Literal[...] | None`".to_string()
        } else {
            "`Literal[None]` can be replaced with a bare `None`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `None`".to_string())
    }
}

/// RUF037
pub(crate) fn redundant_none_literal<'a>(checker: &mut Checker, literal_expr: &'a Expr) {
    if !checker.semantic().seen_typing() {
        return;
    }

    let mut none_exprs: Vec<&Expr> = Vec::new();
    let mut seen_others = false;

    let mut find_none = |expr: &'a Expr, _parent: &'a Expr| {
        if matches!(expr, Expr::NoneLiteral(_)) {
            none_exprs.push(expr);
        } else {
            seen_others = true;
        }
    };

    traverse_literal(&mut find_none, checker.semantic(), literal_expr);

    if none_exprs.is_empty() {
        return;
    }

    // Provide a [`Fix`] when the complete `Literal` can be replaced. Applying the fix
    // can leave an unused import to be fixed by the `unused-import` rule.
    let fix = if seen_others {
        None
    } else {
        Some(Fix::applicable_edit(
            Edit::range_replacement("None".to_string(), literal_expr.range()),
            if checker
                .comment_ranges()
                .has_comments(literal_expr, checker.source())
            {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ))
    };

    for none_expr in none_exprs {
        let mut diagnostic =
            Diagnostic::new(RedundantNoneLiteral { seen_others }, none_expr.range());
        if let Some(ref fix) = fix {
            diagnostic.set_fix(fix.clone());
        }
        checker.diagnostics.push(diagnostic);
    }
}
