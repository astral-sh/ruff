use rustpython_ast::Unaryop;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::compare;
use crate::violations;

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
                            let mut diagnostic = Diagnostic::new(
                                violations::NotInTest,
                                Range::from_located(operand),
                            );
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
                            let mut diagnostic = Diagnostic::new(
                                violations::NotIsTest,
                                Range::from_located(operand),
                            );
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
