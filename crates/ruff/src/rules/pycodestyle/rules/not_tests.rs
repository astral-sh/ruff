use rustpython_parser::ast::{self, Cmpop, Expr, Ranged, Unaryop};

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
    op: Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) {
    if matches!(op, Unaryop::Not) {
        if let Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        }) = operand
        {
            if !matches!(&ops[..], [Cmpop::In | Cmpop::Is]) {
                return;
            }
            for op in ops.iter() {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            let mut diagnostic = Diagnostic::new(NotInTest, operand.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                #[allow(deprecated)]
                                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                    compare(
                                        left,
                                        &[Cmpop::NotIn],
                                        comparators,
                                        checker.generator(),
                                    ),
                                    expr.range(),
                                )));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            let mut diagnostic = Diagnostic::new(NotIsTest, operand.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                #[allow(deprecated)]
                                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                    compare(
                                        left,
                                        &[Cmpop::IsNot],
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
