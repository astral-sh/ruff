use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Scope;

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
/// Checks for the presence of unused private `typing.TypedDict` definitions.
///
/// ## Why is this bad?
/// A private `typing.TypedDict` that is defined but not used is likely a
/// mistake, and should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// import typing
///
///
/// class _UnusedPrivateTypedDict(typing.TypedDict):
///     foo: list[int]
/// ```
///
/// Use instead:
/// ```python
/// import typing
///
///
/// class _UsedPrivateTypedDict(typing.TypedDict):
///     foo: set[str]
///
///
/// def func(arg: _UsedPrivateTypedDict) -> _UsedPrivateTypedDict:
///     ...
/// ```
#[violation]
pub struct UnusedPrivateTypedDict {
    name: String,
}

impl Violation for UnusedPrivateTypedDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypedDict { name } = self;
        format!("Private TypedDict `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for binding in scope
        .binding_ids()
        .map(|binding_id| checker.semantic().binding(binding_id))
    {
        if !(binding.kind.is_assignment() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };
        let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = checker.semantic().stmts[source]
        else {
            continue;
        };
        let [Expr::Name(ast::ExprName { id, .. })] = &targets[..] else {
            continue;
        };
        let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
            continue;
        };
        if !checker.semantic().match_typing_expr(func, "TypeVar") {
            continue;
        }

        diagnostics.push(Diagnostic::new(
            UnusedPrivateTypeVar {
                name: id.to_string(),
            },
            binding.range,
        ));
    }
}

/// PYI046
pub(crate) fn unused_private_protocol(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for binding in scope
        .binding_ids()
        .map(|binding_id| checker.semantic().binding(binding_id))
    {
        if !(binding.kind.is_class_definition() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };
        let Stmt::ClassDef(ast::StmtClassDef { name, bases, .. }) =
            checker.semantic().stmts[source]
        else {
            continue;
        };

        if !bases
            .iter()
            .any(|base| checker.semantic().match_typing_expr(base, "Protocol"))
        {
            continue;
        }

        diagnostics.push(Diagnostic::new(
            UnusedPrivateProtocol {
                name: name.to_string(),
            },
            binding.range,
        ));
    }
}

/// PYI049
pub(crate) fn unused_private_typed_dict(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for binding in scope
        .binding_ids()
        .map(|binding_id| checker.semantic().binding(binding_id))
    {
        if !(binding.kind.is_class_definition() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };
        let Stmt::ClassDef(ast::StmtClassDef { name, bases, .. }) = checker.semantic().stmts[source]
        else {
            continue;
        };

        if !bases
            .iter()
            .any(|base| checker.semantic().match_typing_expr(base, "TypedDict"))
        {
            continue;
        }

        diagnostics.push(Diagnostic::new(
            UnusedPrivateTypedDict {
                name: name.to_string(),
            },
            binding.range,
        ));
    }
}
