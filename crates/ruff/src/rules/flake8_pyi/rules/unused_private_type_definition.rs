use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Binding;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of unused private `TypeVar` declarations.
///
/// ## Why is this bad?
/// A private `TypeVar` that is defined but not used is likely a mistake, and
/// should either be used, made public, or removed to avoid confusion.
///
/// ## Example
/// ```python
/// import typing
///
/// _T = typing.TypeVar("_T")
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

/// ## What it does
/// Checks for the presence of unused private `typing.Protocol` definitions.
///
/// ## Why is this bad?
/// A private `typing.Protocol` that is defined but not used is likely a
/// mistake, and should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// import typing
///
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
/// ```
///
/// Use instead:
/// ```python
/// import typing
///
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
///
///
/// def func(arg: _PrivateProtocol) -> None:
///     ...
/// ```
#[violation]
pub struct UnusedPrivateProtocol {
    name: String,
}

impl Violation for UnusedPrivateProtocol {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateProtocol { name } = self;
        format!("Private protocol `{name}` is never used")
    }
}

/// ## What it does
/// Checks for the presence of unused private `typing.TypeAlias` definitions.
///
/// ## Why is this bad?
/// A private `typing.TypeAlias` that is defined but not used is likely a
/// mistake, and should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// import typing
///
/// _UnusedTypeAlias: typing.TypeAlias = int
/// ```
///
/// Use instead:
/// ```python
/// import typing
///
/// _UsedTypeAlias: typing.TypeAlias = int
///
///
/// def func(arg: _UsedTypeAlias) -> _UsedTypeAlias:
///     ...
/// ```
#[violation]
pub struct UnusedPrivateTypeAlias {
    name: String,
}

impl Violation for UnusedPrivateTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeAlias { name } = self;
        format!("Private TypeAlias `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !(binding.kind.is_assignment() && binding.is_private_declaration()) {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    let Some(source) = binding.source else {
        return None;
    };
    let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = checker.semantic().stmts[source]
    else {
        return None;
    };
    let [Expr::Name(ast::ExprName { id, .. })] = &targets[..] else {
        return None;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return None;
    };
    if !checker.semantic().match_typing_expr(func, "TypeVar") {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateTypeVar { name: id.to_string() },
        binding.range,
    ))
}

/// PYI046
pub(crate) fn unused_private_protocol(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !(binding.kind.is_class_definition() && binding.is_private_declaration()) {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    let Some(source) = binding.source else {
        return None;
    };
    let Stmt::ClassDef(ast::StmtClassDef { name, bases, .. }) = checker.semantic().stmts[source]
    else {
        return None;
    };

    if !bases
        .iter()
        .any(|base| checker.semantic().match_typing_expr(base, "Protocol"))
    {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateProtocol { name: name.to_string() },
        binding.range,
    ))
}

/// PYI047
pub(crate) fn unused_private_type_alias(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !(binding.kind.is_assignment() && binding.is_private_declaration()) {
        return None;
    }
    if binding.is_used() {
        return None;
    }

    let Some(source) = binding.source else {
        return None;
    };
    let Stmt::AnnAssign(ast::StmtAnnAssign { target, annotation, .. }) = checker.semantic().stmts[source]
    else {
        return None
    };
    let Some(ast::ExprName { id, .. }) = target.as_name_expr() else {
        return None;
    };

    if !checker.semantic().match_typing_expr(annotation, "TypeAlias") {
        return None;
    }

    Some(Diagnostic::new(
        UnusedPrivateTypeAlias { name: id.to_string() },
        binding.range,
    ))
}
