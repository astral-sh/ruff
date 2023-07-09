use rustpython_parser::ast::{self, CmpOp, Expr, Ranged, UnaryOp};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

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
/// if not X.B in Y:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// Z = X not in Y
/// if X.B not in Y:
///     pass
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
/// if X is not Y:
///     pass
/// Z = X.B is not Y
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
pub(crate) fn not_tests(
    checker: &mut Checker,
    expr: &Expr,
    op: UnaryOp,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) {
    if matches!(op, UnaryOp::Not) {
        if let Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        }) = operand
        {
            if !matches!(&ops[..], [CmpOp::In | CmpOp::Is]) {
                return;
            }
            for op in ops.iter() {
                match op {
                    CmpOp::In => {
                        if check_not_in {
                            let mut diagnostic = Diagnostic::new(NotInTest, operand.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                                    compare(
                                        left,
                                        &[CmpOp::NotIn],
                                        comparators,
                                        checker.generator(),
                                    ),
                                    expr.range(),
                                )));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    CmpOp::Is => {
                        if check_not_is {
                            let mut diagnostic = Diagnostic::new(NotIsTest, operand.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                                    compare(
                                        left,
                                        &[CmpOp::IsNot],
                                        comparators,
                                        checker.generator(),
                                    ),
                                    expr.range(),
                                )));
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
