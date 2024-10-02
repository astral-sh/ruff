use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for f-strings that contain single quotes and suggests replacing them
/// with `!r` conversion.
///
/// ## Why is this bad?
/// Using `!r` conversion in f-strings is both easier to read and will escape
/// quotes inside the string if they appear.
///
/// ## Example
/// ```python
/// f"'{foo}'"
/// ```
///
/// Use instead:
/// ```python
/// f"{foo!r}"
/// ```
///
/// ## References
/// - [Python documentation: Formatted string literals](https://docs.python.org/3/reference/lexical_analysis.html#f-strings)
#[violation]
pub struct FStringSingleQuotes;

impl Violation for FStringSingleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider replacing f\"'{{foo}}'\" with f\"{{foo!r}}\" which is both easier to read and will escape quotes inside foo if that would appear")
    }
}

/// B907
pub(crate) fn f_string_single_quotes(checker: &mut Checker, expr: &Expr) {
    if let Expr::FString(ast::ExprFString { values, .. }) = expr {
        for value in values {
            if let Expr::FormattedValue(ast::ExprFormattedValue { value, .. }) = value {
                if let Expr::Constant(ast::ExprConstant {
                    value: ast::Constant::Str(s),
                    ..
                }) = value.as_ref()
                {
                    if s.contains('\'') {
                        checker.diagnostics.push(Diagnostic::new(
                            FStringSingleQuotes,
                            expr.range(),
                        ));
                    }
                }
            }
        }
    }
}
