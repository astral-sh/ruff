use std::fmt;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Decimal` calls passing a float literal.
///
/// ## Why is this bad?
/// Float literals have limited precision that can lead to unexpected results.
/// The `Decimal` class is designed to handle numbers with fixed-point precision,
/// so a string literal should be used instead.
///
/// ## Example
///
/// ```python
/// num = Decimal(1.2345)
/// ```
///
/// Use instead:
/// ```python
/// num = Decimal("1.2345")
/// ```
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe because it changes the underlying value
/// of the `Decimal` instance that is constructed. This can lead to unexpected
/// behavior if your program relies on the previous value (whether deliberately or not).
#[violation]
pub struct DecimalFromFloatLiteral;

impl AlwaysFixableViolation for DecimalFromFloatLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`Decimal()` called with float literal argument")
    }

    fn fix_title(&self) -> String {
        "Use a string literal instead".to_string()
    }
}

/// RUF032: `Decimal()` called with float literal argument
pub(crate) fn decimal_from_float_literal_syntax(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(arg) = call.arguments.args.first() else {
        return;
    };

    if let Some(float) = extract_float_literal(arg, Sign::Positive) {
        if checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["decimal", "Decimal"])
            })
        {
            let diagnostic = Diagnostic::new(DecimalFromFloatLiteral, arg.range()).with_fix(
                fix_float_literal(arg.range(), float, checker.locator(), checker.stylist()),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Sign {
    Positive,
    Negative,
}

impl Sign {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "",
            Self::Negative => "-",
        }
    }

    const fn flip(self) -> Self {
        match self {
            Self::Negative => Self::Positive,
            Self::Positive => Self::Negative,
        }
    }
}

impl fmt::Display for Sign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Copy, Clone)]
struct Float {
    /// The range of the float excluding the sign.
    /// E.g. for `+--+-+-4.3`, this will be the range of `4.3`
    value_range: TextRange,
    /// The resolved sign of the float (either `-` or `+`)
    sign: Sign,
}

fn extract_float_literal(arg: &ast::Expr, sign: Sign) -> Option<Float> {
    match arg {
        ast::Expr::NumberLiteral(number_literal_expr) if number_literal_expr.value.is_float() => {
            Some(Float {
                value_range: arg.range(),
                sign,
            })
        }
        ast::Expr::UnaryOp(ast::ExprUnaryOp {
            operand,
            op: ast::UnaryOp::UAdd,
            ..
        }) => extract_float_literal(operand, sign),
        ast::Expr::UnaryOp(ast::ExprUnaryOp {
            operand,
            op: ast::UnaryOp::USub,
            ..
        }) => extract_float_literal(operand, sign.flip()),
        _ => None,
    }
}

fn fix_float_literal(
    original_range: TextRange,
    float: Float,
    locator: &Locator,
    stylist: &Stylist,
) -> Fix {
    let quote = stylist.quote();
    let Float { value_range, sign } = float;
    let float_value = locator.slice(value_range);
    let content = format!("{quote}{sign}{float_value}{quote}");
    Fix::unsafe_edit(Edit::range_replacement(content, original_range))
}
