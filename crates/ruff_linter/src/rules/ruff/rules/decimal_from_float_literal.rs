use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_codegen::Stylist;
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

    if !is_arg_float_literal(arg) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["decimal", "Decimal"]))
    {
        let diagnostic =
            Diagnostic::new(DecimalFromFloatLiteral, arg.range()).with_fix(fix_float_literal(
                arg.range(),
                &checker.generator().expr(arg),
                checker.stylist(),
            ));
        checker.diagnostics.push(diagnostic);
    }
}

fn is_arg_float_literal(arg: &ast::Expr) -> bool {
    match arg {
        ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Float(_),
            ..
        }) => true,
        ast::Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => is_arg_float_literal(operand),
        _ => false,
    }
}

fn fix_float_literal(range: TextRange, float_literal: &str, stylist: &Stylist) -> Fix {
    let content = format!("{quote}{float_literal}{quote}", quote = stylist.quote());
    Fix::unsafe_edit(Edit::range_replacement(content, range))
}
