use ruff_python_ast::{Expr, ExprLambda};

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct ReimplementedContainerBuiltin {
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
pub(crate) fn reimplemented_container_builtin(checker: &Checker, expr: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
    } = expr;

    if parameters.is_some() {
        return;
    }

    let container = match &**body {
        Expr::List(list) if list.is_empty() => Container::List,
        Expr::Dict(dict) if dict.is_empty() => Container::Dict,
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(ReimplementedContainerBuiltin { container }, expr.range());
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
            container.as_str(),
            expr.start(),
            checker.semantic(),
        )?;
        let binding_edit = Edit::range_replacement(binding, expr.range());
        Ok(Fix::safe_edits(binding_edit, import_edit))
    });
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Container {
    List,
    Dict,
}

impl Container {
    const fn as_str(self) -> &'static str {
        match self {
            Container::List => "list",
            Container::Dict => "dict",
        }
    }
}

impl std::fmt::Display for Container {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.write_str(self.as_str())
    }
}
