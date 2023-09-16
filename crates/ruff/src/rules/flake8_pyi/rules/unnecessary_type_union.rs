use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, rules::flake8_pyi::helpers::traverse_union};

/// ## What it does
/// Checks for the presence of multiple `type`s in a union.
///
/// ## Why is this bad?
/// The `type` built-in function accepts unions, and it is
/// clearer to explicitly specify them as a single `type`.
///
/// ## Example
/// ```python
/// field: type[int] | type[float]
/// ```
///
/// Use instead:
/// ```python
/// field: type[int | float]
/// ```
#[violation]
pub struct UnnecessaryTypeUnion {
    members: Vec<String>,
    is_pep604_union: bool,
}

impl Violation for UnnecessaryTypeUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let union_str = if self.is_pep604_union {
            format!("{}", self.members.join(" | "))
        } else {
            format!("Union[{}]", self.members.join(", "))
        };

        format!(
            "Multiple `type` members in a union. Combine them into one, e.g., `type[{union_str}]`."
        )
    }
}

/// PYI055
pub(crate) fn unnecessary_type_union<'a>(checker: &mut Checker, union: &'a Expr) {
    // The `|` operator isn't always safe to allow to runtime-evaluated annotations.
    if checker.semantic().execution_context().is_runtime() {
        return;
    }

    let mut type_exprs = Vec::new();

    // Check if `union` is a PEP604 union (e.g. `float | int`) or a `typing.Union[float, int]`
    let is_pep604_union = !union.as_subscript_expr().is_some_and(|subscript| {
        checker
            .semantic()
            .match_typing_expr(&subscript.value, "Union")
    });

    let mut collect_type_exprs = |expr: &'a Expr, _| {
        let Some(subscript) = expr.as_subscript_expr() else {
            return;
        };
        if checker
            .semantic()
            .resolve_call_path(subscript.value.as_ref())
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["" | "builtins", "type"]))
        {
            type_exprs.push(&subscript.slice);
        }
    };

    traverse_union(&mut collect_type_exprs, checker.semantic(), union, None);

    if type_exprs.len() > 1 {
        checker.diagnostics.push(Diagnostic::new(
            UnnecessaryTypeUnion {
                members: type_exprs
                    .into_iter()
                    .map(|type_expr| checker.locator().slice(type_expr.as_ref()).to_string())
                    .collect(),
                is_pep604_union,
            },
            union.range(),
        ));
    }
}
