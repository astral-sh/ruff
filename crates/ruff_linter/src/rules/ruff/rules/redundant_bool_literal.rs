use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use bitflags::bitflags;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Literal[True, False]` type annotations.
///
/// ## Why is this bad?
/// The `bool` type has exactly two constant instances: `True` and `False`. Writing
/// `Literal[True, False]` in the context of type annotations could be replaced by a
/// bare `bool` annotations. Static type checkers such as [mypy] treat them as
/// equivalent.
///
/// However, `bool` is not strictly equivalent to `Literal[True, False]`, as `bool` is
/// a subclass of `int`, so this rule might not apply if the type annotations are used
/// in a numerical context as well.
///
/// ## Example
/// ```python
/// Literal[True, False]
/// Literal[True, False, "hello", "world"]
/// ```
///
/// Use instead:
/// ```python
/// bool
/// Literal["hello", "world"] | bool
/// ```
///
/// ## Fix safety
/// The fix for this rule is marked as unsafe when the `Literal` contains comments.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
/// - [Python documentation: Boolean type - `bool`](https://docs.python.org/3/library/stdtypes.html#boolean-type-bool)
///
/// [mypy](https://github.com/python/mypy/blob/master/mypy/typeops.py#L985)
#[violation]
pub struct RedundantBoolLiteral {
    seen_others: bool,
}

impl Violation for RedundantBoolLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.seen_others {
            "`Literal[True, False, ...]` can be replaced with `Literal[...] | bool`".to_string()
        } else {
            "`Literal[True, False]` can be replaced with a bare `bool`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `bool`".to_string())
    }
}

/// RUF038
pub(crate) fn redundant_bool_literal<'a>(checker: &mut Checker, literal_expr: &'a Expr) {
    if !checker.semantic().seen_typing() {
        return;
    }

    let mut seen_expr = BooleanLiteral::empty();

    let mut find_bools = |expr: &'a Expr, _parent: &'a Expr| {
        let expr_type = match expr {
            Expr::BooleanLiteral(boolean_expr) => {
                if boolean_expr.value {
                    BooleanLiteral::TRUE
                } else {
                    BooleanLiteral::FALSE
                }
            }
            _ => BooleanLiteral::OTHER,
        };
        seen_expr.insert(expr_type);
    };

    traverse_literal(&mut find_bools, checker.semantic(), literal_expr);

    if !seen_expr.contains(BooleanLiteral::TRUE | BooleanLiteral::FALSE) {
        return;
    }

    let seen_others = seen_expr.contains(BooleanLiteral::OTHER);

    let mut diagnostic =
        Diagnostic::new(RedundantBoolLiteral { seen_others }, literal_expr.range());

    // Provide a [`Fix`] when the complete `Literal` can be replaced. Applying the fix
    // can leave an unused import to be fixed by the `unused-import` rule.
    if !seen_others {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement("bool".to_string(), literal_expr.range()),
            if checker
                .comment_ranges()
                .has_comments(literal_expr, checker.source())
            {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }

    checker.diagnostics.push(diagnostic);
}

bitflags! {
    #[derive(Default, Debug)]
    struct BooleanLiteral: u8 {
        const TRUE = 1 << 0;
        const FALSE = 1 << 1;
        const OTHER = 1 << 2;
    }
}
