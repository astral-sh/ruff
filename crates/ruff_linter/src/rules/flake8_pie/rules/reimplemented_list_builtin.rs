use ruff_python_ast::{self as ast, Expr, ExprLambda};

use ruff_diagnostics::{AutofixKind, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for lambdas that can be replaced with the `list` builtin.
///
/// ## Why is this bad?
/// Using `list` builtin is more readable.
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
/// ```
///
/// ## References
/// - [Python documentation: `list`](https://docs.python.org/3/library/functions.html#func-list)
#[violation]
pub struct ReimplementedListBuiltin;

impl Violation for ReimplementedListBuiltin {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `list` over useless lambda")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with `list`".to_string())
    }
}

/// PIE807
pub(crate) fn reimplemented_list_builtin(checker: &mut Checker, expr: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
    } = expr;

    if parameters.is_none() {
        if let Expr::List(ast::ExprList { elts, .. }) = body.as_ref() {
            if elts.is_empty() {
                let mut diagnostic = Diagnostic::new(ReimplementedListBuiltin, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    if checker.semantic().is_builtin("list") {
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            "list".to_string(),
                            expr.range(),
                        )));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
