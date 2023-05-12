use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for usages of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
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
#[violation]
pub struct ExplicitFStringTypeConversion;

impl AlwaysAutofixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use conversion in f-string")
    }

    fn autofix_title(&self) -> String {
        "Replace f-string function call with conversion".to_string()
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(
    checker: &mut Checker,
    expr: &Expr,
    formatted_value: &Expr,
    conversion: ast::Int,
) {
    // Skip if there's already a conversion flag.
    if conversion != ast::ConversionFlag::None as u32 {
        return;
    }

    let ExprKind::Call(ast::ExprCall {
        func,
        args,
        keywords,
    }) = &formatted_value.node else {
        return;
    };

    // Can't be a conversion otherwise.
    if args.len() != 1 || !keywords.is_empty() {
        return;
    }

    let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else {
        return;
    };

    if !matches!(id.as_str(), "str" | "repr" | "ascii") {
        return;
    };

    if !checker.ctx.is_builtin(id) {
        return;
    }

    let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, formatted_value.range());

    if checker.patch(diagnostic.kind.rule()) {
        // Replace the call node with its argument and a conversion flag.
        let mut conv_expr = expr.clone();
        let ExprKind::FormattedValue(ast::ExprFormattedValue {
            ref mut conversion,
            ref mut value,
            ..
        }) = conv_expr.node else {
            return;
        };

        *conversion = match id.as_str() {
            "ascii" => ast::Int::new(ast::ConversionFlag::Ascii as u32),
            "str" => ast::Int::new(ast::ConversionFlag::Str as u32),
            "repr" => ast::Int::new(ast::ConversionFlag::Repr as u32),
            &_ => unreachable!(),
        };

        value.node = args[0].node.clone();

        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            unparse_expr(&conv_expr, checker.stylist),
            formatted_value
                .range()
                .sub_start(TextSize::from(1))
                .add_end(TextSize::from(1)),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
