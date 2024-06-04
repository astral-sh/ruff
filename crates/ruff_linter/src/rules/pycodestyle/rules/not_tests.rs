use crate::fix::edits::pad;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::generate_comparison;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for membership tests using `not {element} in {collection}`.
///
/// ## Why is this bad?
/// Testing membership with `{element} not in {collection}` is more readable.
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

impl AlwaysFixableViolation for NotInTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Test for membership should be `not in`")
    }

    fn fix_title(&self) -> String {
        "Convert to `not in`".to_string()
    }
}

/// ## What it does
/// Checks for identity comparisons using `not {foo} is {bar}`.
///
/// ## Why is this bad?
/// According to [PEP8], testing for an object's identity with `is not` is more
/// readable.
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
///
/// [PEP8]: https://peps.python.org/pep-0008/#programming-recommendations
#[violation]
pub struct NotIsTest;

impl AlwaysFixableViolation for NotIsTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Test for object identity should be `is not`")
    }

    fn fix_title(&self) -> String {
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

    match &**ops {
        [CmpOp::In] => {
            if checker.enabled(Rule::NotInTest) {
                let mut diagnostic = Diagnostic::new(NotInTest, unary_op.operand.range());
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    pad(
                        generate_comparison(
                            left,
                            &[CmpOp::NotIn],
                            comparators,
                            unary_op.into(),
                            checker.comment_ranges(),
                            checker.locator(),
                        ),
                        unary_op.range(),
                        checker.locator(),
                    ),
                    unary_op.range(),
                )));
                checker.diagnostics.push(diagnostic);
            }
        }
        [CmpOp::Is] => {
            if checker.enabled(Rule::NotIsTest) {
                let mut diagnostic = Diagnostic::new(NotIsTest, unary_op.operand.range());
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    pad(
                        generate_comparison(
                            left,
                            &[CmpOp::IsNot],
                            comparators,
                            unary_op.into(),
                            checker.comment_ranges(),
                            checker.locator(),
                        ),
                        unary_op.range(),
                        checker.locator(),
                    ),
                    unary_op.range(),
                )));
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => {}
    }
}
