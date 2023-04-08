use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Expr, ExprKind};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DuplicatesInUnion {
    pub duplicate_id: String,
}

impl AlwaysAutofixableViolation for DuplicatesInUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Duplicate `{}` in union", self.duplicate_id)
    }

    fn autofix_title(&self) -> String {
        format!("Remove latter `{}` from union", self.duplicate_id)
    }
}

///PYI016
pub fn duplicates_in_union(checker: &mut Checker, left: &Expr, right: &Expr) {
    if let ExprKind::Name { id: id1, ctx: _ } = &left.node {
        if let ExprKind::Name { id: id2, ctx: _ } = &right.node {
            if id1 == id2 {
                // Violation found, create diagnostic & fix
                let mut diagnostic = Diagnostic::new(
                    DuplicatesInUnion {
                        duplicate_id: id1.to_string(),
                    },
                    Range::from(right),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // We want to delete the "|" character as well as the duplicate
                    // value, so delete from the end of "left" to the end of "right"
                    diagnostic.set_fix(Edit::deletion(
                        left.end_location.unwrap(),
                        right.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
