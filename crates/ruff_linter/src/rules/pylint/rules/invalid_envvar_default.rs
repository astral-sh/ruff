use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `os.getenv` calls with invalid default values.
///
/// ## Why is this bad?
/// If an environment variable is set, `os.getenv` will return its value as
/// a string. If the environment variable is _not_ set, `os.getenv` will
/// return `None`, or the default value if one is provided.
///
/// If the default value is not a string or `None`, then it will be
/// inconsistent with the return type of `os.getenv`, which can lead to
/// confusing behavior.
///
/// ## Example
/// ```python
/// import os
///
/// int(os.getenv("FOO", 1))
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// int(os.getenv("FOO", "1"))
/// ```
#[violation]
pub struct InvalidEnvvarDefault;

impl Violation for InvalidEnvvarDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid type for environment variable default; expected `str` or `None`")
    }
}

fn is_valid_default(expr: &Expr) -> bool {
    // We can't infer the types of these defaults, so assume they're valid.
    if matches!(
        expr,
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_)
    ) {
        return true;
    }

    // Allow string concatenation.
    if let Expr::BinOp(ast::ExprBinOp {
        left,
        right,
        op: Operator::Add,
        range: _,
    }) = expr
    {
        return is_valid_default(left) && is_valid_default(right);
    }

    // Allow string formatting.
    if let Expr::BinOp(ast::ExprBinOp {
        left,
        op: Operator::Mod,
        ..
    }) = expr
    {
        return is_valid_default(left);
    }

    // Otherwise, the default must be a string or `None`.
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str { .. } | Constant::None { .. },
            ..
        }) | Expr::FString(_)
    )
}

/// PLW1508
pub(crate) fn invalid_envvar_default(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "getenv"]))
    {
        // Find the `default` argument, if it exists.
        let Some(expr) = call.arguments.find_argument("default", 1) else {
            return;
        };

        if !is_valid_default(expr) {
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidEnvvarDefault, expr.range()));
        }
    }
}
