use ruff_python_ast::{self as ast, Expr, ExprLambda};

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lambdas that can be replaced with the `list` or `dict` builtins.
///
/// ## Why is this bad?
/// Using container builtins are more succinct and idiomatic than wrapping
/// the literal in a lambda.
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
/// ## References
/// - [Python documentation: `list`](https://docs.python.org/3/library/functions.html#func-list)
#[violation]
pub struct ReimplementedContainerBuiltin {
    container: Container,
}

impl Violation for ReimplementedContainerBuiltin {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { container } = self;
        format!("Prefer `{container}` over useless lambda")
    }

    fn fix_title(&self) -> Option<String> {
        let Self { container } = self;
        Some(format!("Replace with `lambda` with `{container}`"))
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
        let container = match body.as_ref() {
            Expr::List(ast::ExprList { elts, .. }) if elts.is_empty() => Some(Container::List),
            Expr::Dict(ast::ExprDict { values, .. }) if values.is_empty() => Some(Container::Dict),
            _ => None,
        };
        if let Some(container) = container {
            let mut diagnostic =
                Diagnostic::new(ReimplementedContainerBuiltin { container }, expr.range());
            if checker.semantic().is_builtin(container.as_str()) {
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    container.to_string(),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Container {
    List,
    Dict,
}

impl Container {
    fn as_str(self) -> &'static str {
        match self {
            Container::List => "list",
            Container::Dict => "dict",
        }
    }
}

impl std::fmt::Display for Container {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Container::List => fmt.write_str("list"),
            Container::Dict => fmt.write_str("dict"),
        }
    }
}
