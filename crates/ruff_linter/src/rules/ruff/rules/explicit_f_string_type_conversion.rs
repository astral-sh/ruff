use std::fmt::Display;

use anyhow::Result;

use libcst_native::{LeftParen, ParenthesizedNode, RightParen};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, Expr, OperatorPrecedence};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::helpers::space;
use crate::cst::matchers::{
    match_call_mut, match_formatted_string, match_formatted_string_expression, transform_expression,
};
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
///
/// Note that, in many cases, calling `str()` within an f-string is
/// unnecessary and can be removed entirely, as the value will be converted
/// to a string automatically, the notable exception being for classes that
/// implement a custom `__format__` method.
///
/// ## Example
/// ```python
/// a = "some string"
/// f"{repr(a)}"
/// ```
///
/// Use instead:
/// ```python
/// a = "some string"
/// f"{a!r}"
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.267")]
pub(crate) struct ExplicitFStringTypeConversion;

impl Violation for ExplicitFStringTypeConversion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use explicit conversion flag".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with conversion flag".to_string())
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(checker: &Checker, f_string: &ast::FString) {
    for (index, element) in f_string.elements.iter().enumerate() {
        let Some(ast::InterpolatedElement {
            expression,
            conversion,
            ..
        }) = element.as_interpolation()
        else {
            continue;
        };

        // Skip if there's already a conversion flag.
        if !conversion.is_none() {
            continue;
        }

        let Expr::Call(call) = expression.as_ref() else {
            continue;
        };

        let Some(conversion) = checker
            .semantic()
            .resolve_builtin_symbol(&call.func)
            .and_then(Conversion::from_str)
        else {
            continue;
        };
        let arg = match conversion {
            // Handles the cases: `f"{str(object=arg)}"` and `f"{str(arg)}"`
            Conversion::Str if call.arguments.len() == 1 => {
                let Some(arg) = call.arguments.find_argument_value("object", 0) else {
                    continue;
                };
                arg
            }
            Conversion::Str | Conversion::Repr | Conversion::Ascii => {
                // Can't be a conversion otherwise.
                if !call.arguments.keywords.is_empty() {
                    continue;
                }

                // Can't be a conversion otherwise.
                let [arg] = call.arguments.args.as_ref() else {
                    continue;
                };
                arg
            }
        };

        // Suppress lint for starred expressions.
        if arg.is_starred_expr() {
            return;
        }

        // Don't report diagnostic for f-string with debug text.
        if element
            .as_interpolation()
            .is_some_and(|interpolation| interpolation.debug_text.is_some())
        {
            return;
        }

        let mut diagnostic =
            checker.report_diagnostic(ExplicitFStringTypeConversion, expression.range());

        diagnostic.try_set_fix(|| {
            convert_call_to_conversion_flag(checker, conversion, f_string, index, arg)
        });
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    checker: &Checker,
    conversion: Conversion,
    f_string: &ast::FString,
    index: usize,
    arg: &Expr,
) -> Result<Fix> {
    let source_code = checker.locator().slice(f_string);
    transform_expression(source_code, checker.stylist(), |mut expression| {
        let formatted_string = match_formatted_string(&mut expression)?;
        // Replace the formatted call expression at `index` with a conversion flag.
        let formatted_string_expression =
            match_formatted_string_expression(&mut formatted_string.parts[index])?;
        let call = match_call_mut(&mut formatted_string_expression.expression)?;

        formatted_string_expression.conversion = Some(conversion.as_str());

        if starts_with_brace(checker, arg) {
            formatted_string_expression.whitespace_before_expression = space();
        }

        formatted_string_expression.expression = if needs_paren_expr(arg) {
            call.args[0]
                .value
                .clone()
                .with_parens(LeftParen::default(), RightParen::default())
        } else {
            call.args[0].value.clone()
        };

        Ok(expression)
    })
    .map(|output| Fix::safe_edit(Edit::range_replacement(output, f_string.range())))
}

fn starts_with_brace(checker: &Checker, arg: &Expr) -> bool {
    checker
        .tokens()
        .in_range(arg.range())
        .iter()
        // Skip the trivia tokens
        .find(|token| !token.kind().is_trivia())
        .is_some_and(|token| matches!(token.kind(), TokenKind::Lbrace))
}

fn needs_paren(precedence: OperatorPrecedence) -> bool {
    precedence <= OperatorPrecedence::Lambda
}

fn needs_paren_expr(arg: &Expr) -> bool {
    // Generator expressions need to be parenthesized in f-string expressions
    if let Some(generator) = arg.as_generator_expr() {
        return !generator.parenthesized;
    }

    // Check precedence for other expressions
    needs_paren(OperatorPrecedence::from_expr(arg))
}

/// Represents the three built-in Python conversion functions that can be replaced
/// with f-string conversion flags.
#[derive(Copy, Clone)]
enum Conversion {
    Ascii,
    Str,
    Repr,
}

impl Conversion {
    fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "ascii" => Self::Ascii,
            "str" => Self::Str,
            "repr" => Self::Repr,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Conversion::Ascii => "a",
            Conversion::Str => "s",
            Conversion::Repr => "r",
        }
    }
}

impl Display for Conversion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
