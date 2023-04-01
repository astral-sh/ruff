use rustpython_parser::ast::{Cmpop, Expr, ExprKind, Unaryop};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::compare;

/// ## What it does
/// Checks for negative comparison using `not {foo} in {bar}`.
///
/// ## Why is this bad?
/// Negative comparison should be done using `not in`.
///
/// ## Example
/// ```python
/// Z = not X in Y
/// if not X.B in Y:\n    pass
///
/// ```
///
/// Use instead:
/// ```python
/// if x not in y:\n    pass
/// assert (X in Y or X is Z)
///
/// ```
#[violation]
pub struct NotInTest;

impl AlwaysAutofixableViolation for NotInTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Test for membership should be `not in`")
    }

    fn autofix_title(&self) -> String {
        "Convert to `not in`".to_string()
    }
}

/// ## What it does
/// Checks for negative comparison using `not {foo} is {bar}`.
///
/// ## Why is this bad?
/// Negative comparison should be done using `is not`.
///
/// ## Example
/// ```python
/// if not X is Y:
///     pass
/// Z = not X.B is Y
/// ```
///
/// Use instead:
/// ```python
/// if not (X in Y):
///     pass
/// zz = x is not y
/// ```
#[violation]
pub struct NotIsTest;

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
                            let mut diagnostic = Diagnostic::new(NotInTest, Range::from(operand));
                            if checker.patch(diagnostic.kind.rule()) && should_fix {
                                diagnostic.set_fix(Edit::replacement(
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
                            let mut diagnostic = Diagnostic::new(NotIsTest, Range::from(operand));
                            if checker.patch(diagnostic.kind.rule()) && should_fix {
                                diagnostic.set_fix(Edit::replacement(
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
