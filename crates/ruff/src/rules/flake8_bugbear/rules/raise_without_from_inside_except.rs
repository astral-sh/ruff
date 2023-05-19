use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_stdlib::str::is_lower;

use crate::checkers::ast::Checker;

#[violation]
pub struct RaiseWithoutFromInsideExcept;

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
pub(crate) fn raise_without_from_inside_except(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    for (range, exc, cause) in raises {
        if cause.is_none() {
            if let Some(exc) = exc {
                match exc {
                    Expr::Name(ast::ExprName { id, .. }) if is_lower(id) => {}
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
