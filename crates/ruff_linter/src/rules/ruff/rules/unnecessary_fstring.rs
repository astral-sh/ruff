use crate::checkers::ast::{Checker};
use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, ConversionFlag, Expr, ExprCall, ExprName};
use ruff_text_size::Ranged;



/// # What it does
/// Checks for f-strings that consist of a single format item.
///
/// # Why is this bad?
/// Wrapping the expression in extra quotes and curly braces makes the
/// code harder to read.
///
/// # Example
/// ```python
/// print(f"{_("Hello, world!")}")
/// ```
///
/// Use instead:
/// ```python
/// print(_("Hello, world!"))
/// ```
///
/// # Options
/// - TODO

#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryFString;

impl Violation for UnnecessaryFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        "f-string consisting only of a single replacement field".to_string()
    }
}

/// RUF056
pub(crate) fn unnecessary_fstring(
    checker: &mut Checker, expr: &ast::ExprFString
) {
    if let Some(f_string) = expr.value.as_single() {
        if f_string.elements.len() != 1 {
            return;
        }
        if let Some(format_expr) = f_string.elements[0].as_expression() {
            // TODO: can we be smarter about when we emit str()? There are many cases where we don't need it, but currently it's emitted unconditionally.
            let function = match format_expr.conversion {
                ConversionFlag::None | ConversionFlag::Str => "str",
                ConversionFlag::Ascii => "ascii",
                ConversionFlag::Repr => "repr",
            };
            let fix = Fix::safe_edits(
                Edit::replacement(format!("{function}("), expr.start(), format_expr.expression.start()),
                [Edit::replacement(")".to_string(), format_expr.expression.end(), expr.end())],
            );
            checker.diagnostics.push(Diagnostic::new(UnnecessaryFString, f_string.range()).with_fix(fix));
        }
    }
}

