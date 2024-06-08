use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_text_size::Ranged;

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
/// This rule is not enforced for some built-in exceptions that are commonly
/// raised with a message and would be unusual to subclass, such as
/// `NotImplementedError`.
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

/// TRY003
pub(crate) fn raise_vanilla_args(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = expr
    else {
        return;
    };

    let Some(arg) = args.first() else {
        return;
    };

    // Ignore some built-in exceptions that don't make sense to subclass, like
    // `NotImplementedError`.
    if checker
        .semantic()
        .match_builtin_expr(func, "NotImplementedError")
    {
        return;
    }

    if contains_message(arg) {
        checker
            .diagnostics
            .push(Diagnostic::new(RaiseVanillaArgs, expr.range()));
    }
}

/// Returns `true` if an expression appears to be an exception message (i.e., a string with
/// some whitespace).
fn contains_message(expr: &Expr) -> bool {
    match expr {
        Expr::FString(ast::ExprFString { value, .. }) => {
            for f_string_part in value {
                match f_string_part {
                    ast::FStringPart::Literal(literal) => {
                        if literal.chars().any(char::is_whitespace) {
                            return true;
                        }
                    }
                    ast::FStringPart::FString(f_string) => {
                        for literal in f_string.elements.literals() {
                            if literal.chars().any(char::is_whitespace) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            if value.chars().any(char::is_whitespace) {
                return true;
            }
        }
        _ => {}
    }

    false
}
