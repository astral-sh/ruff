use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::compare;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct NotInTest;
);
impl AlwaysAutofixableViolation for NotInTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Test for membership should be `not in`")
    }

    fn autofix_title(&self) -> String {
        "Convert to `not in`".to_string()
    }
}

define_violation!(
    pub struct NotIsTest;
);
impl AlwaysAutofixableViolation for NotIsTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Test for object identity should be `is not`")
    }

    fn autofix_title(&self) -> String {
        "Convert to `is not`".to_string()
    }
}

/// E713, E714
pub fn not_tests(
    checker: &mut Checker,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) {
    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare {
            left,
            ops,
            comparators,
            ..
        } = &operand.node
        {
            let should_fix = ops.len() == 1;
            for op in ops.iter() {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            let mut diagnostic =
                                Diagnostic::new(NotInTest, Range::from_located(operand));
                            if checker.patch(diagnostic.kind.rule()) && should_fix {
                                diagnostic.amend(Fix::replacement(
                                    compare(left, &[Cmpop::NotIn], comparators, checker.stylist),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            let mut diagnostic =
                                Diagnostic::new(NotIsTest, Range::from_located(operand));
                            if checker.patch(diagnostic.kind.rule()) && should_fix {
                                diagnostic.amend(Fix::replacement(
                                    compare(left, &[Cmpop::IsNot], comparators, checker.stylist),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
