use rustpython_parser::ast::{self, Constant, Expr, Keyword, Operator, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `os.getenv` calls with an invalid `key` argument.
///
/// ## Why is this bad?
/// `os.getenv` only supports strings as the first argument (`key`).
///
/// If the provided argument is not a string, `os.getenv` will throw a
/// `TypeError` at runtime.
///
/// ## Example
/// ```python
/// os.getenv(1)
/// ```
///
/// Use instead:
/// ```python
/// os.getenv("1")
/// ```
#[violation]
pub struct InvalidEnvvarValue;

impl Violation for InvalidEnvvarValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid type for initial `os.getenv` argument; expected `str`")
    }
}

fn is_valid_key(expr: &Expr) -> bool {
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
        return is_valid_key(left) && is_valid_key(right);
    }

    // Allow string formatting.
    if let Expr::BinOp(ast::ExprBinOp {
        left,
        op: Operator::Mod,
        ..
    }) = expr
    {
        return is_valid_key(left);
    }

    // Otherwise, the default must be a string.
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str { .. },
            ..
        }) | Expr::JoinedStr(_)
    )
}

/// PLE1507
pub(crate) fn invalid_envvar_value(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["os", "getenv"])
        })
    {
        // Find the `key` argument, if it exists.
        let Some(expr) = args.get(0).or_else(|| {
            keywords
                .iter()
                .find(|keyword| keyword.arg.as_ref().map_or(false, |arg| arg == "key"))
                .map(|keyword| &keyword.value)
        }) else {
            return;
        };

        if !is_valid_key(expr) {
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidEnvvarValue, expr.range()));
        }
    }
}
