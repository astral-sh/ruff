use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for list, set, and dictionary comprehensions without filters used
/// as boolean conditions in `assert` statements.
///
/// ## Why is this bad?
/// Asserting a comprehension checks whether the resulting collection is
/// nonempty, not whether its elements are truthy. Without an `if` filter,
/// the assertion depends on whether the iteration produces any items and
/// ignores the truthiness of the computed elements.
///
/// Use `all` or `any` to check the elements, or check the iterable directly
/// if the intent is to verify that it is nonempty. Comprehensions with filters
/// are allowed because they can intentionally check whether any items match
/// a condition.
///
/// ## Example
/// ```python
/// assert [name.startswith("test_") for name in names]
/// ```
///
/// Use instead:
/// ```python
/// assert all(name.startswith("test_") for name in names)
/// ```
///
/// Unlike a collection, `all` is true for an empty iterable. If the iterable
/// must also be nonempty, check that separately.
///
/// No automatic fix is offered because the intended check may be `all`, `any`,
/// or a check of the iterable itself, and changing the expression may affect
/// which elements are evaluated.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct AssertUnfilteredComprehension;

impl Violation for AssertUnfilteredComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Assert checks comprehension emptiness, not element truthiness".to_string()
    }
}

/// `assert-unfiltered-comprehension`
pub(crate) fn assert_unfiltered_comprehension(checker: &Checker, test: &Expr) {
    match test {
        Expr::ListComp(ast::ExprListComp { generators, .. })
        | Expr::SetComp(ast::ExprSetComp { generators, .. })
        | Expr::DictComp(ast::ExprDictComp { generators, .. }) => {
            if generators.iter().all(|generator| generator.ifs.is_empty()) {
                checker.report_diagnostic(AssertUnfilteredComprehension, test.range());
            }
        }
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
            for value in values {
                assert_unfiltered_comprehension(checker, value);
            }
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            operand,
            ..
        }) => assert_unfiltered_comprehension(checker, operand),
        Expr::Named(ast::ExprNamed { value, .. }) => {
            assert_unfiltered_comprehension(checker, value);
        }
        Expr::If(ast::ExprIf {
            test, body, orelse, ..
        }) => {
            assert_unfiltered_comprehension(checker, test);
            assert_unfiltered_comprehension(checker, body);
            assert_unfiltered_comprehension(checker, orelse);
        }
        _ => {}
    }
}
