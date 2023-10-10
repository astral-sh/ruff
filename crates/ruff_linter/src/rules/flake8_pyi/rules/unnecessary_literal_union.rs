use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_pyi::helpers::traverse_union;

/// ## What it does
/// Checks for the presence of multiple literal types in a union.
///
/// ## Why is this bad?
/// Literal types accept multiple arguments and it is clearer to specify them
/// as a single literal.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// field: Literal[1] | Literal[2]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// field: Literal[1, 2]
/// ```
#[violation]
pub struct UnnecessaryLiteralUnion {
    members: Vec<String>,
}

impl AlwaysFixableViolation for UnnecessaryLiteralUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Multiple literal members in a union. Use a single literal, e.g. `Literal[{}]`",
            self.members.join(", ")
        )
    }

    fn fix_title(&self) -> String {
        format!("Replace with a single `Literal`.")
    }
}

/// PYI030
pub(crate) fn unnecessary_literal_union<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut literal_exprs = Vec::new();

    // Adds a member to `literal_exprs` if it is a `Literal` annotation
    let mut collect_literal_expr = |expr: &'a Expr, _| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                literal_exprs.push(slice);
            }
        }
    };

    // Traverse the union, collect all literal members
    traverse_union(&mut collect_literal_expr, checker.semantic(), expr, None);

    // Raise a violation if more than one
    if literal_exprs.len() > 1 {
        let literal_members: Vec<String> = literal_exprs
            .into_iter()
            .map(|expr| checker.locator().slice(expr.as_ref()).to_string())
            .collect();

        let mut diagnostic = Diagnostic::new(
            UnnecessaryLiteralUnion {
                members: literal_members.clone(),
            },
            expr.range(),
        );

        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                format!("Literal[{}]", literal_members.join(", ")),
                expr.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}
