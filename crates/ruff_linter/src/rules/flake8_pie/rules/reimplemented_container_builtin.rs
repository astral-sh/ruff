use ruff_python_ast::{self as ast, Expr, ExprLambda};

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lambdas that can be replaced with the `list` builtin.
/// If [preview] mode is enabled, then we will also look for lambdas
/// that can be replaced with the `dict` builtin.
///
/// ## Why is this bad?
/// Using container builtins is more readable.
///
/// ## Example
/// ```python
/// from dataclasses import dataclass, field
///
///
/// @dataclass
/// class Foo:
///     bar: list[int] = field(default_factory=lambda: [])
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass, field
///
///
/// @dataclass
/// class Foo:
///     bar: list[int] = field(default_factory=list)
///     baz: dict[str, int] = field(default_factory=dict)
/// ```
///
///
/// If [preview]
/// ## References
/// - [Python documentation: `list`](https://docs.python.org/3/library/functions.html#func-list)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[violation]
pub struct ReimplementedContainerBuiltin(&'static str);

impl Violation for ReimplementedContainerBuiltin {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `{}` over useless lambda", self.0)
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with lambda with `{}`", self.0))
    }
}

/// PIE807
pub(crate) fn reimplemented_container_builtin(checker: &mut Checker, expr: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
    } = expr;

    if parameters.is_none() {
        let builtin = match body.as_ref() {
            Expr::List(ast::ExprList { elts, .. }) if elts.is_empty() => Some("list"),
            Expr::Dict(ast::ExprDict { values, .. })
                if values.is_empty() & checker.settings.preview.is_enabled() =>
            {
                Some("dict")
            }
            _ => None,
        };
        if let Some(builtin) = builtin {
            let mut diagnostic =
                Diagnostic::new(ReimplementedContainerBuiltin(builtin), expr.range());
            if checker.semantic().is_builtin(builtin) {
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    builtin.to_string(),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
