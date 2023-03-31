use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for long exception messages that are not defined in the exception
/// class itself.
///
/// ## Why is this bad?
/// By formatting an exception message at the `raise` site, the exception class
/// becomes less reusable, and may now raise inconsistent messages depending on
/// where it is raised.
///
/// If the exception message is instead defined within the exception class, it
/// will be consistent across all `raise` invocations.
///
/// ## Example
/// ```python
/// class CantBeNegative(Exception):
///     pass
///
///
/// def foo(x):
///     if x < 0:
///         raise CantBeNegative(f"{x} is negative")
/// ```
///
/// Use instead:
/// ```python
/// class CantBeNegative(Exception):
///     def __init__(self, number):
///         super().__init__(f"{number} is negative")
///
///
/// def foo(x):
///     if x < 0:
///         raise CantBeNegative(x)
/// ```
#[violation]
pub struct RaiseVanillaArgs;

impl Violation for RaiseVanillaArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid specifying long messages outside the exception class")
    }
}

fn any_string<F>(expr: &Expr, predicate: F) -> bool
where
    F: (Fn(&str) -> bool) + Copy,
{
    match &expr.node {
        ExprKind::JoinedStr { values } => {
            for value in values {
                if any_string(value, predicate) {
                    return true;
                }
            }
        }
        ExprKind::Constant {
            value: Constant::Str(val),
            ..
        } => {
            if predicate(val.as_str()) {
                return true;
            }
        }
        _ => {}
    }

    false
}

/// TRY003
pub fn raise_vanilla_args(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call { args, .. } = &expr.node {
        if let Some(arg) = args.first() {
            if any_string(arg, |part| part.chars().any(char::is_whitespace)) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(RaiseVanillaArgs, expr.range()));
            }
        }
    }
}
