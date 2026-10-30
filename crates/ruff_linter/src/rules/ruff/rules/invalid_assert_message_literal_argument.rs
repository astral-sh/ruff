use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::fields as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for invalid use of literals in assert message arguments.
///
/// ## Why is this bad?
/// An assert message which is a non-string literal was likely intended
/// to be used in a comparison assertion, rather than as a message.
///
/// ## Example
/// ```python
/// fruits = ["apples", "plums", "pears"]
/// fruits.filter(lambda fruit: fruit.startswith("p"))
/// assert len(fruits), 2  # True unless the list is empty
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["apples", "plums", "pears"]
/// fruits.filter(lambda fruit: fruit.startswith("p"))
/// assert len(fruits) == 2
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0", category = Category::Suspicious)]
pub(crate) struct InvalidAssertMessageLiteralArgument;

impl Violation for InvalidAssertMessageLiteralArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Non-string literal used as assert message".to_string()
    }
}

/// RUF040
pub(crate) fn invalid_assert_message_literal_argument(
    checker: &Checker,
    stmt: &ast::StmtAssert<'_>,
) {
    let Some(message) = stmt.msg else {
        return;
    };

    if !matches!(
        message.classify(),
        flat::ExprKind::NumberLiteral(_)
            | flat::ExprKind::BooleanLiteral(_)
            | flat::ExprKind::NoneLiteral(_)
            | flat::ExprKind::EllipsisLiteral(_)
            | flat::ExprKind::BytesLiteral(_)
    ) {
        return;
    }

    checker.report_diagnostic(InvalidAssertMessageLiteralArgument, message.range());
}
