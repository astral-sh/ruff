use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct ComplexIfTestInStubs;

impl Violation for ComplexIfTestInStubs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test must be a simple comparison against sys.platform or sys.version_info")
    }
}

/// PYI002
pub(crate) fn complex_if_test_in_stubs(checker: &mut Checker, test: &Expr) {
    if let Expr::Compare(comp_expr) = test {
        if comp_expr.left.is_attribute_expr() && comp_expr.comparators.len() == 1 {
            if let Some(attribute_call_path) = checker
                .semantic()
                .resolve_call_path(comp_expr.left.as_ref())
            {
                if matches!(
                    attribute_call_path.as_slice(),
                    ["sys", "platform" | "version_info"]
                ) {
                    return;
                }
            }
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(ComplexIfTestInStubs, test.range()));
}
