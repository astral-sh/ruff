use rustpython_parser::ast::{ExprKind, Stmt};

use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::str::is_lower;

use crate::ast::helpers::RaiseStatementVisitor;
use crate::ast::visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RaiseWithoutFromInsideExcept;
);
impl Violation for RaiseWithoutFromInsideExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Within an `except` clause, raise exceptions with `raise ... from err` or `raise ... \
             from None` to distinguish them from errors in exception handling"
        )
    }
}

/// B904
pub fn raise_without_from_inside_except(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor::walk_body(&mut visitor, body);
        visitor.raises
    };

    for (range, exc, cause) in raises {
        if cause.is_none() {
            if let Some(exc) = exc {
                match &exc.node {
                    ExprKind::Name { id, .. } if is_lower(id) => {}
                    _ => {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(RaiseWithoutFromInsideExcept, range));
                    }
                }
            }
        }
    }
}
