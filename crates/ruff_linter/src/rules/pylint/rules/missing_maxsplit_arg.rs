use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for access to the first or last element of `str.split()` without 
/// `maxsplit=1`
///
/// ## Why is this bad?
/// Calling `str.split()` without maxsplit set splits on every delimiter in the 
/// string. When accessing only the first or last element of the result, it 
/// would be more efficient to only split once.
///
/// ## Example
/// ```python
/// url = "www.example.com"
/// prefix = url.split(".")[0]
/// ```
///
/// Use instead:
/// ```python
/// url = "www.example.com"
/// prefix = url.split(".", maxsplit=1)[0]
/// ```

#[derive(ViolationMetadata)]
pub(crate) struct MissingMaxsplitArg;

impl Violation for MissingMaxsplitArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Accessing only the first or last element of `str.split()` without setting `maxsplit=1`".to_string()
    }
}

/// PLC0207
pub(crate) fn missing_maxsplit_arg(checker: &Checker, value: &ast::Expr, slice: &ast::Expr, expr: &ast::Expr) {
    // Check the sliced expression is a function
    let ast::Expr::Call(ast::ExprCall { func, arguments: ast::Arguments { args, keywords, .. }, .. }) = value else {
        return;
    };

    // Check the slice index is either 0 or -1 (first or last value)
    let index = match slice {
        ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(number_value),
            ..
        }) => number_value.as_i64(),
        ast::Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            ..
        }) => match operand.as_ref() {
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(number_value),
                ..
            }) => number_value.as_i64().map(|number| -number),
            _ => return,
        },
        _ => return,
    };

    if !matches!(index, Some(0 | -1)) {
        return;
    }


    if let ast::Expr::Attribute(ast::ExprAttribute {attr, value, .. }) = func.as_ref() {
        // Check the function is "split" or "rsplit"
        let attr = attr.as_str();
        if !matches!(attr, "split" | "rsplit") {
            return;
        }

        // Check the function is called on a string
        if let ast::Expr::Name(name) = value.as_ref() {
            let semantic = checker.semantic();

            let Some(binding_id) = semantic.only_binding(name) else {
                return; 
            };
            let binding = semantic.binding(binding_id);

            if !typing::is_string(binding, semantic) {
                return;
            } 
        } else if let ast::Expr::StringLiteral(_) = value.as_ref() {
            // pass
        } else {
            return;
        }

    } else {
        return;
    }

    // Check the function does not have kwarg maxsplit=1
    if keywords.iter().any(|kw| {
        kw.arg.as_deref() == Some("maxsplit")
            && matches!(
                kw.value,
                ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(ast::Int::ONE),
                    ..
                })
            )
    }) {
        return;
    }

    // Check the function does not have arg[1]=1
    if matches!(
        args.get(1),
        Some(ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(ast::Int::ONE),
            ..
        }))
    ) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(MissingMaxsplitArg, expr.range()));
}
