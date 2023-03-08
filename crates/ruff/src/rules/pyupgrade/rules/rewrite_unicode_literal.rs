use rustpython_parser::ast::{Expr, Location};

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

#[violation]
pub struct RewriteUnicodeLiteral;

impl AlwaysAutofixableViolation for RewriteUnicodeLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove unicode literals from strings")
    }

    fn autofix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub fn rewrite_unicode_literal(checker: &mut Checker, expr: &Expr, kind: Option<&str>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut diagnostic = Diagnostic::new(RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch((&diagnostic.kind).into()) {
                diagnostic.amend(Fix::deletion(
                    expr.location,
                    Location::new(expr.location.row(), expr.location.column() + 1),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
