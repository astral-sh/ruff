use ruff_python_ast::{self as ast, Constant, Expr, Keyword};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary `dict` kwargs.
///
/// ## Why is this bad?
/// If the `dict` keys are valid identifiers, they can be passed as keyword
/// arguments directly.
///
/// ## Example
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(**{"bar": 2}))  # prints 3
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(bar=2))  # prints 3
/// ```
///
/// ## References
/// - [Python documentation: Dictionary displays](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
#[violation]
pub struct UnnecessaryDictKwargs;

impl Violation for UnnecessaryDictKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }
}

/// PIE804
pub(crate) fn unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.arg.is_none() {
            if let Expr::Dict(ast::ExprDict { keys, .. }) = &kw.value {
                // ensure foo(**{"bar-bar": 1}) doesn't error
                if keys.iter().all(|expr| expr.as_ref().is_some_and( is_valid_kwarg_name)) ||
                    // handle case of foo(**{**bar})
                    (keys.len() == 1 && keys[0].is_none())
                {
                    let diagnostic = Diagnostic::new(UnnecessaryDictKwargs, expr.range());
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// Return `true` if a key is a valid keyword argument name.
fn is_valid_kwarg_name(key: &Expr) -> bool {
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    }) = key
    {
        is_identifier(value)
    } else {
        false
    }
}
