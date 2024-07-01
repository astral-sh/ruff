use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::{is_immutable_func, is_mutable_expr, is_mutable_func};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of mutable objects as `ContextVar` defaults.
///
/// ## Why is this bad?
///
/// The `ContextVar` default is evaluated once, when the `ContextVar` is defined.
///
/// The same mutable object is then shared across all `.get()` method calls to
/// the `ContextVar`. If the object is modified, those modifications will persist
/// across calls, which can lead to unexpected behavior.
///
/// Instead, prefer to use immutable data structures; or, take `None` as a
/// default, and initialize a new mutable object inside for each call using the
/// `.set()` method.
///
/// Types outside the standard library can be marked as immutable with the
/// [`lint.flake8-bugbear.extend-immutable-calls`] configuration option.
///
/// ## Example
/// ```python
/// from contextvars import ContextVar
///
///
/// cv: ContextVar[list] = ContextVar("cv", default=[])
/// ```
///
/// Use instead:
/// ```python
/// from contextvars import ContextVar
///
///
/// cv: ContextVar[list | None] = ContextVar("cv", default=None)
///
/// ...
///
/// if cv.get() is None:
///     cv.set([])
/// ```
///
/// ## Options
/// - `lint.flake8-bugbear.extend-immutable-calls`
///
/// ## References
/// - [Python documentation: [`contextvars` — Context Variables](https://docs.python.org/3/library/contextvars.html)
#[violation]
pub struct MutableContextvarDefault;

impl Violation for MutableContextvarDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for `ContextVar` defaults")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `None`; initialize with `.set()``".to_string())
    }
}

/// B039
pub(crate) fn mutable_contextvar_default(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::CONTEXTVARS) {
        return;
    }

    let Some(default) = call
        .arguments
        .find_keyword("default")
        .map(|keyword| &keyword.value)
    else {
        return;
    };

    let extend_immutable_calls: Vec<QualifiedName> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .collect();

    if (is_mutable_expr(default, checker.semantic())
        || matches!(
            default,
            Expr::Call(ast::ExprCall { func, .. })
                if !is_mutable_func(func, checker.semantic())
                    && !is_immutable_func(func, checker.semantic(), &extend_immutable_calls)))
        && checker
            .semantic()
            .resolve_qualified_name(&call.func)
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["contextvars", "ContextVar"])
            })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(MutableContextvarDefault, default.range()));
    }
}
