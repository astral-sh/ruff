use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_literal;
use ruff_text_size::Ranged;

use bitflags::bitflags;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Literal[True, False]` type annotations.
///
/// ## Why is this bad?
/// `Literal[True, False]` can be replaced with `bool` in type annotations,
/// which has the same semantic meaning but is more concise and readable.
///
/// `bool` type has exactly two constant instances: `True` and `False`. Static
/// type checkers such as [mypy] treat `Literal[True, False]` as equivalent to
/// `bool` in a type annotation.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// x: Literal[True, False]
/// y: Literal[True, False, "hello", "world"]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// x: bool
/// y: Literal["hello", "world"] | bool
/// ```
///
/// ## Fix safety
/// The fix for this rule is marked as unsafe, as it may change the semantics of the code.
/// Specifically:
///
/// - Type checkers may not treat `bool` as equivalent when overloading boolean arguments
///   with `Literal[True]` and `Literal[False]` (see, e.g., [#14764] and [#5421]).
/// - `bool` is not strictly equivalent to `Literal[True, False]`, as `bool` is
///   a subclass of `int`, and this rule may not apply if the type annotations are used
///   in a numeric context.
///
/// Further, the `Literal` slice may contain trailing-line comments which the fix would remove.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
/// - [Python documentation: Boolean type - `bool`](https://docs.python.org/3/library/stdtypes.html#boolean-type-bool)
///
/// [mypy]: https://github.com/python/mypy/blob/master/mypy/typeops.py#L985
/// [#14764]: https://github.com/python/mypy/issues/14764
/// [#5421]: https://github.com/microsoft/pyright/issues/5421
#[derive(ViolationMetadata)]
pub(crate) struct RedundantBoolLiteral {
    seen_others: bool,
}

impl Violation for RedundantBoolLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.seen_others {
            "`Literal[True, False, ...]` can be replaced with `Literal[...] | bool`".to_string()
        } else {
            "`Literal[True, False]` can be replaced with `bool`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(if self.seen_others {
            "Replace with `Literal[...] | bool`".to_string()
        } else {
            "Replace with `bool`".to_string()
        })
    }
}

/// RUF038
pub(crate) fn redundant_bool_literal<'a>(checker: &Checker, literal_expr: &'a Expr) {
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
        if checker.semantic().has_builtin_binding("bool") {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                "bool".to_string(),
                literal_expr.range(),
            )));
        }
    }

    checker.report_diagnostic(diagnostic);
}

bitflags! {
    #[derive(Default, Debug)]
    struct BooleanLiteral: u8 {
        const TRUE = 1 << 0;
        const FALSE = 1 << 1;
        const OTHER = 1 << 2;
    }
}
