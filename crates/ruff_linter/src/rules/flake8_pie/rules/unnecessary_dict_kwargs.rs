use itertools::Itertools;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_python_ast::{self as ast, Expr, Keyword};

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

impl AlwaysFixableViolation for UnnecessaryDictKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }

    fn fix_title(&self) -> String {
        format!("Remove unnecessary kwargs")
    }
}

/// PIE804
pub(crate) fn unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.arg.is_some() {
            continue;
        }

        let Expr::Dict(ast::ExprDict { keys, values, .. }) = &kw.value else {
            continue;
        };

        // Ex) `foo(**{**bar})`
        if matches!(keys.as_slice(), [None]) {
            let mut diagnostic = Diagnostic::new(UnnecessaryDictKwargs, expr.range());

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                format!("**{}", checker.locator().slice(values[0].range())),
                kw.range(),
            )));

            checker.diagnostics.push(diagnostic);
            continue;
        }

        // Ensure that every keyword is a valid keyword argument (e.g., avoid errors for cases like
        // `foo(**{"bar-bar": 1})`).
        let kwargs = keys
            .iter()
            .filter_map(|key| key.as_ref().and_then(as_kwarg))
            .collect::<Vec<_>>();
        if kwargs.len() != keys.len() {
            continue;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryDictKwargs, expr.range());

        if values.is_empty() {
            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(kw.start(), kw.end())));
        } else {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                kwargs
                    .iter()
                    .zip(values.iter())
                    .map(|(kwarg, value)| {
                        format!("{}={}", kwarg, checker.locator().slice(value.range()))
                    })
                    .join(", "),
                kw.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Return `Some` if a key is a valid keyword argument name, or `None` otherwise.
fn as_kwarg(key: &Expr) -> Option<&str> {
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = key {
        if is_identifier(value) {
            return Some(value.as_str());
        }
    }
    None
}
