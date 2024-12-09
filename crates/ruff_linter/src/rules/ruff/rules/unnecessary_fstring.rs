use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, ConversionFlag};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for f-strings that consist of a single format item.
///
/// ## Why is this bad?
/// Wrapping the expression in extra quotes and curly braces makes the
/// code harder to read.
///
/// ## Example
/// ```python
/// f"{variable}"
/// ```
///
/// Use instead (if `variable` needs to be converted to a string explicitly):
/// ```python
/// str(variable)
/// ```
/// Or (if `variable` already has the desired string-like properties):
/// ```python
/// variable
/// ```

#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryFString {
    conversion: ConversionFlag,
}

fn conversion_function(conversion: &ConversionFlag) -> &str {
    match conversion {
        ConversionFlag::None | ConversionFlag::Str => "str",
        ConversionFlag::Ascii => "ascii",
        ConversionFlag::Repr => "repr",
    }
}

impl Violation for UnnecessaryFString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;
    #[derive_message_formats]
    fn message(&self) -> String {
        "f-string consisting only of a single replacement field".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Remove f-string or replace it with a call of the {} function",
            conversion_function(&self.conversion)
        ))
    }
}

/// RUF056
pub(crate) fn unnecessary_fstring(checker: &mut Checker, expr: &ast::ExprFString) {
    if let Some(f_string) = expr.value.as_single() {
        if f_string.elements.len() != 1 {
            return;
        }
        if let Some(format_expr) = f_string.elements[0].as_expression() {
            if format_expr.format_spec.is_some() {
                return;
            }
            // Edit is unsafe because:
            // - str(foo) may behave differently when `str` is overridden in the local scope.
            // - Empty format specifiers producing the same result as `str` is "A general convention" according to https://docs.python.org/3.13/library/string.html#format-specification-mini-language
            let fix = Fix::unsafe_edits(
                Edit::replacement(
                    format!("{}(", conversion_function(&format_expr.conversion)),
                    expr.start(),
                    format_expr.expression.start(),
                ),
                [Edit::replacement(
                    ")".to_string(),
                    format_expr.expression.end(),
                    expr.end(),
                )],
            );
            checker.diagnostics.push(
                Diagnostic::new(
                    UnnecessaryFString {
                        conversion: format_expr.conversion,
                    },
                    f_string.range(),
                )
                .with_fix(fix),
            );
        }
    }
}
