use crate::define_simple_autofix_violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::find_useless_f_strings;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_simple_autofix_violation!(
    FStringMissingPlaceholders,
    "f-string without any placeholders",
    "Remove extraneous `f` prefix"
);

/// F541
pub fn f_string_missing_placeholders(expr: &Expr, values: &[Expr], checker: &mut Checker) {
    if !values
        .iter()
        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
    {
        for (prefix_range, tok_range) in find_useless_f_strings(expr, checker.locator) {
            let mut diagnostic = Diagnostic::new(FStringMissingPlaceholders, tok_range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::deletion(
                    prefix_range.location,
                    prefix_range.end_location,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
