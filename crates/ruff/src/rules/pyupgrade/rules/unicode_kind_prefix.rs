use rustpython_parser::ast::{Expr, Location};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UnicodeKindPrefix;

impl AlwaysAutofixableViolation for UnicodeKindPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove unicode literals from strings")
    }

    fn autofix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub fn unicode_kind_prefix(checker: &mut Checker, expr: &Expr, kind: Option<&str>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut diagnostic = Diagnostic::new(UnicodeKindPrefix, Range::from(expr));
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Edit::deletion(
                    expr.location,
                    Location::new(expr.location.row(), expr.location.column() + 1),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
