use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Binding;
use rustpython_parser::ast::{self, Expr, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of unused private `TypeVar` declarations.
///
/// ## Why is this bad?
/// A private `TypeVar` that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// ## Example
/// ```python
/// import typing
/// _T = typing.TypeVar("_T")
/// ```
///
/// Use instead:
/// ```python
/// import typing
/// _T = typing.TypeVar("_T")
///
/// def func(arg: _T) -> _T: ...
/// ```
#[violation]
pub struct UnusedPrivateTypeVar {
    name: String,
}

impl Violation for UnusedPrivateTypeVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeVar { name } = self;
        format!("Private TypeVar `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !binding.kind.is_assignment() && !binding.is_private_variable() {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    let Some(source) = binding.source else {
        return None;
    };
    let Stmt::Assign(ast::StmtAssign {targets, value, ..}) = checker.semantic().stmts[source] else {
        return None;
    };
    let [Expr::Name(ast::ExprName {id, ..})] = &targets[..] else {
        return None;
    };
    let Expr::Call(ast::ExprCall {func, ..}) = value.as_ref() else {
        return None;
    };
    if !checker.semantic().match_typing_expr(func, "TypeVar") {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateTypeVar {
            name: id.to_string(),
        },
        binding.range,
    ))
}
