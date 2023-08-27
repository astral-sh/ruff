use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle::helpers::generate_comparison;

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
pub(crate) fn not_tests(checker: &mut Checker, unary_op: &ast::ExprUnaryOp) {
    if !unary_op.op.is_not() {
        return;
    }

    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = unary_op.operand.as_ref()
    else {
        return;
    };

    match ops.as_slice() {
        [CmpOp::In] => {
            if checker.enabled(Rule::NotInTest) {
                let mut diagnostic = Diagnostic::new(NotInTest, unary_op.operand.range());
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                        generate_comparison(
                            left,
                            &[CmpOp::NotIn],
                            comparators,
                            unary_op.into(),
                            checker.locator(),
                        ),
                        unary_op.range(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        [CmpOp::Is] => {
            if checker.enabled(Rule::NotIsTest) {
                let mut diagnostic = Diagnostic::new(NotIsTest, unary_op.operand.range());
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                        generate_comparison(
                            left,
                            &[CmpOp::IsNot],
                            comparators,
                            unary_op.into(),
                            checker.locator(),
                        ),
                        unary_op.range(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => {}
    }
}
