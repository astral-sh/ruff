use ruff_python_ast::Expr;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary dictionary unpacking operators (`**`).
///
/// ## Why is this bad?
/// Unpacking a dictionary into another dictionary is redundant. The unpacking
/// operator can be removed, making the code more readable.
///
/// ## Example
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, **{"C": 3}}
/// ```
///
/// Use instead:
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, "C": 3}
/// ```
///
/// ## References
/// - [Python documentation: Dictionary displays](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
#[violation]
pub struct UnnecessarySpread;

impl Violation for UnnecessarySpread {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary spread `**`")
    }
}

/// PIE800
pub(crate) fn unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let Expr::Dict(_) = value {
                let diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
