use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `os.getenv` calls with an invalid first argument
///
/// ## Why is this bad?
/// `os.getenv` only supports string type arguments.
///
/// If the provided argument is not a string, `os.getenv` will not function properly.
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

/// PLE1507
pub fn invalid_envvar_value(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["os", "getenv"])
    {
        // Get the first argument for `getenv`
        if let Some(expr) = args.get(0) {
            // Ignoring types that are inferred, only do direct constants
            if !matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::Str { .. },
                    ..
                } | ExprKind::Name { .. }
                    | ExprKind::Attribute { .. }
                    | ExprKind::Subscript { .. }
                    | ExprKind::Call { .. }
            ) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(InvalidEnvvarValue, Range::from(expr)));
            }
        }
    }
}
