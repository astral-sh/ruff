use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Expr, ExprKind};

use crate::checkers::ast::Checker;

#[violation]
pub struct FStringConversion;
impl AlwaysAutofixableViolation for FStringConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use conversion in f-string")
    }

    fn autofix_title(&self) -> String {
        "Replace f-string function call with conversion".to_string()
    }
}

/// RUF010
pub(crate) fn f_string_conversion(
    checker: &mut Checker,
    expr: &Expr,
    formatted_value: &Expr,
    conversion: ast::Int,
) {
    // Make sure we're in an f-string
    if !checker.ctx.in_f_string {
        return;
    }
    // Skip if there's already a conversion
    if conversion != ast::ConversionFlag::None as u32 {
        return;
    }

    if let ExprKind::Call(ast::ExprCall {
        func,
        args,
        keywords,
    }) = &formatted_value.node
    {
        if args.len() != 1 || !keywords.is_empty() {
            // Can't be a conversion otherwise
            return;
        }

        let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else {
            return;
        };

        match id.as_str() {
            "ascii" | "str" | "repr" => {
                if !checker
                    .ctx
                    .find_binding(id)
                    .map(|binding| &binding.kind)
                    .expect("Must at least find builtin")
                    .is_builtin()
                {
                    // The call is to a different than the builtin
                    return;
                }

                let mut diagnostic = Diagnostic::new(FStringConversion, formatted_value.range());

                // Replace the call node with its argument and a conversion
                let mut conv_expr = expr.clone();
                if let ExprKind::FormattedValue(ast::ExprFormattedValue {
                    ref mut conversion,
                    ref mut value,
                    ..
                }) = conv_expr.node
                {
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

                    checker.diagnostics.push(diagnostic);
                }
            }
            _ => {}
        }
    }
}
