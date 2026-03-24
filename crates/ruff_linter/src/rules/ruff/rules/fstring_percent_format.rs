use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::Violation;

/// ## What it does
/// Checks for uses of the `%` operator on f-strings.
///
/// ## Why is this bad?
/// F-strings already support interpolation via `{...}` expressions.
/// Using the `%` operator on an f-string is almost certainly a mistake,
/// since the f-string's interpolation and `%`-formatting serve the same
/// purpose. This typically indicates that the developer intended to use
/// either an f-string or `%`-formatting, but not both.
///
/// ## Example
/// ```python
/// f"{name}" % name
/// f"hello %s %s" % (first, second)
/// ```
///
/// Use instead:
/// ```python
/// f"{name}"
/// f"hello {first} {second}"
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct FStringPercentFormat;

impl Violation for FStringPercentFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`%` operator used on an f-string".to_string()
    }
}

/// RUF073
pub(crate) fn fstring_percent_format(checker: &Checker, expr: &ast::ExprBinOp) {
    let ast::ExprBinOp {
        left,
        op: Operator::Mod,
        ..
    } = expr
    else {
        return;
    };

    if matches!(left.as_ref(), Expr::FString(_)) {
        checker.report_diagnostic(FStringPercentFormat, expr.range());
    }
}
