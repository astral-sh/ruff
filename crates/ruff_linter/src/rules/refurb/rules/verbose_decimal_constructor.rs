use regex::Regex;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_trivia::PythonWhitespace;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `Decimal` constructor that can be made more succinct.
/// This includes unnecessary string literal or special literal of float.
///
/// ## Why is this bad?
/// This will make code longer and harder to read.
///
/// ## Example
/// ```python
/// Decimal("0")
/// Decimal(float("Infinity"))
/// ```
///
/// Use instead:
/// ```python
/// Decimal(0)
/// Decimal("Infinity")
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as the `Decimal` could be user-defined
/// function or constructor, which is not intended for the fixtures like above.
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
#[violation]
pub struct VerboseDecimalConstructor {
    replace_old: String,
    replace_new: String,
}

impl Violation for VerboseDecimalConstructor {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Verbose expression in `Decimal` constructor")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Replace {} with {}",
            self.replace_old, self.replace_new
        ))
    }
}

/// FURB157
pub(crate) fn verbose_decimal_constructor(checker: &mut Checker, call: &ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["decimal", "Decimal"]))
    {
        return;
    }
    let ast::Arguments { args, keywords, .. } = &call.arguments;
    // Decimal accepts arguments of the form Decimal(value='0', context=None).
    let Some(value) = args.first().or_else(|| {
        keywords
            .iter()
            .find(|&keyword| keyword.arg.as_ref().map(ast::Identifier::as_str) == Some("value"))
            .map(|keyword| &keyword.value)
    }) else {
        return;
    };

    let decimal_constructor = checker.locator().slice(call.func.range());

    let diagnostic = match value {
        Expr::StringLiteral(ast::ExprStringLiteral {
            value: str_literal, ..
        }) => {
            let trimmed = str_literal
                .to_str()
                .trim_whitespace()
                .trim_start_matches('+');
            let integer_string = Regex::new(r"^([+\-]?)0*(\d+)$").unwrap();
            if !integer_string.is_match(trimmed) {
                return;
            };

            let intg = integer_string.replace(trimmed, "$1$2").into_owned();

            let mut diagnostic = Diagnostic::new(
                VerboseDecimalConstructor {
                    replace_old: format!("{}(\"{}\")", decimal_constructor, str_literal.to_str()),
                    replace_new: format!("{decimal_constructor}({intg})"),
                },
                call.range(),
            );

            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                format!("{decimal_constructor}({intg})"),
                call.range(),
            )));

            diagnostic
        }
        Expr::Call(
            floatcall @ ast::ExprCall {
                func, arguments, ..
            },
        ) => {
            let Some(func_name) = func.as_name_expr() else {
                return;
            };
            if func_name.id != "float" {
                return;
            };
            if !checker.semantic().is_builtin(&func_name.id) {
                return;
            };

            if arguments.args.len() != 1 || arguments.keywords.len() > 0 {
                return;
            };
            let Some(value_float) = arguments.args[0].as_string_literal_expr() else {
                return;
            };
            let value_float_str = value_float.value.to_str();
            if !matches!(
                value_float_str.to_lowercase().as_str(),
                "inf" | "-inf" | "infinity" | "-infinity" | "nan"
            ) {
                return;
            }

            let mut diagnostic = Diagnostic::new(
                VerboseDecimalConstructor {
                    replace_old: format!("float(\"{value_float_str}\")"),
                    replace_new: format!("\"{value_float_str}\""),
                },
                call.range(),
            );

            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                format!("\"{value_float_str}\""),
                floatcall.range(),
            )));

            diagnostic
        }
        _ => {
            return;
        }
    };

    checker.diagnostics.push(diagnostic);
}
