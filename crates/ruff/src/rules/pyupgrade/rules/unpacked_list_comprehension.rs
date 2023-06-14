use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UnpackedListComprehension;

impl AlwaysAutofixableViolation for UnpackedListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace unpacked list comprehension with a generator expression")
    }

    fn autofix_title(&self) -> String {
        "Replace with generator expression".to_string()
    }
}

/// UP027
pub(crate) fn unpacked_list_comprehension(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let Some(target) = targets.get(0) else {
        return;
    };

    if !target.is_tuple_expr() {
        return;
    }

    let Expr::ListComp(ast::ExprListComp {
        elt,
        generators,
        range: _,
    }) = value else {
        return;
    };

    if generators.iter().any(|generator| generator.is_async) || contains_await(elt) {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnpackedListComprehension, value.range());
    if checker.patch(diagnostic.kind.rule()) {
        let existing = checker.locator.slice(value.range());

        let mut content = String::with_capacity(existing.len());
        content.push('(');
        content.push_str(&existing[1..existing.len() - 1]);
        content.push(')');
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            content,
            value.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the [`Expr`] contains an `await` expression.
fn contains_await(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
}
