use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `env.getenv` calls with invalid default values.
///
/// ## Why is this bad?
/// If an environment variable is set, `env.getenv` will return its value as
/// a string. If the environment variable is _not_ set, `env.getenv` will
/// return `None`, or the default value if one is provided.
///
/// If the default value is not a string or `None`, then it will be
/// inconsistent with the return type of `env.getenv`, which can lead to
/// confusing behavior.
///
/// ## Example
/// ```python
/// int(env.getenv("FOO", 1))
/// ```
///
/// Use instead:
/// ```python
/// int(env.getenv("FOO", "1"))
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
        expr.node,
        ExprKind::Name { .. }
            | ExprKind::Attribute { .. }
            | ExprKind::Subscript { .. }
            | ExprKind::Call { .. }
    ) {
        return true;
    }

    // Allow string concatenation.
    if let ExprKind::BinOp {
        left,
        right,
        op: Operator::Add,
    } = &expr.node
    {
        return is_valid_default(left) && is_valid_default(right);
    }

    // Allow string formatting.
    if let ExprKind::BinOp {
        left,
        op: Operator::Mod,
        ..
    } = &expr.node
    {
        return is_valid_default(left);
    }

    // Otherwise, the default must be a string or `None`.
    matches!(
        expr.node,
        ExprKind::Constant {
            value: Constant::Str { .. } | Constant::None { .. },
            ..
        } | ExprKind::JoinedStr { .. }
    )
}

/// PLW1508
pub fn invalid_envvar_default(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["os", "getenv"])
    {
        // Find the `default` argument, if it exists.
        let Some(expr) = args.get(1).or_else(|| {
            keywords
                .iter()
                .find(|keyword| keyword.node.arg.as_ref().map_or(false, |arg| arg == "default"))
                .map(|keyword| &keyword.node.value)
        }) else {
            return;
        };

        if !is_valid_default(expr) {
            checker
                .diagnostics
                .push(Diagnostic::new(InvalidEnvvarDefault, Range::from(expr)));
        }
    }
}
