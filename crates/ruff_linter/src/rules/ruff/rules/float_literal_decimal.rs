use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Decimal` calls passing a float literal.
///
/// ## Why is this bad?
/// Float literals are non-deterministic and can lead to unexpected results. The `Decimal` class is designed to handle
/// numbers with fixed-point precision, so a string literal should be used instead.
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
#[violation]
pub struct FloatLiteralDecimal;

impl AlwaysFixableViolation for FloatLiteralDecimal {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"`Decimal()` called with float literal argument"#)
    }

    fn fix_title(&self) -> String {
        "Use a string literal instead".into()
    }
}

/// RUF032: `Decimal()` called with float literal argument
pub(crate) fn float_literal_decimal_syntax(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["decimal", "Decimal"]))
    {
        return;
    }
    if let Some(arg) = call.arguments.args.first() {
        if let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Float(_),
            ..
        }) = arg
        {
            let diagnostic = Diagnostic::new(FloatLiteralDecimal, arg.range()).with_fix(
                fix_float_literal(arg.range(), &checker.generator().expr(arg)),
            );
            checker.diagnostics.push(diagnostic);
        } else if let ast::Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) = arg {
            if let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Float(_),
                ..
            }) = operand.as_ref()
            {
                let diagnostic = Diagnostic::new(FloatLiteralDecimal, arg.range()).with_fix(
                    fix_float_literal(arg.range(), &checker.generator().expr(arg)),
                );
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

fn fix_float_literal(range: TextRange, float_literal: &str) -> Fix {
    let content = format!("\"{float_literal}\"");
    Fix::unsafe_edit(Edit::range_replacement(content, range))
}
