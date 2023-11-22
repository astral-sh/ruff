use std::fmt;

use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum VarKind {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
}

impl fmt::Display for VarKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarKind::TypeVar => fmt.write_str("TypeVar"),
            VarKind::ParamSpec => fmt.write_str("ParamSpec"),
            VarKind::TypeVarTuple => fmt.write_str("TypeVarTuple"),
        }
    }
}

/// ## What it does
/// Checks that type `TypeVar`, `ParamSpec`, and `TypeVarTuple` definitions in
/// stubs are prefixed with `_`.
///
/// ## Why is this bad?
/// By prefixing type parameters with `_`, we can avoid accidentally exposing
/// names internal to the stub.
///
/// ## Example
/// ```python
/// from typing import TypeVar
///
/// T = TypeVar("T")
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypeVar
///
/// _T = TypeVar("_T")
/// ```
#[violation]
pub struct UnprefixedTypeParam {
    kind: VarKind,
}

impl Violation for UnprefixedTypeParam {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnprefixedTypeParam { kind } = self;
        format!("Name of private `{kind}` must start with `_`")
    }
}

/// PYI001
pub(crate) fn prefix_type_params(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };
    if let Expr::Name(ast::ExprName { id, .. }) = target {
        if id.starts_with('_') {
            return;
        }
    };

    if let Expr::Call(ast::ExprCall { func, .. }) = value {
        let Some(kind) = checker
            .semantic()
            .resolve_call_path(func)
            .and_then(|call_path| {
                if checker
                    .semantic()
                    .match_typing_call_path(&call_path, "ParamSpec")
                {
                    Some(VarKind::ParamSpec)
                } else if checker
                    .semantic()
                    .match_typing_call_path(&call_path, "TypeVar")
                {
                    Some(VarKind::TypeVar)
                } else if checker
                    .semantic()
                    .match_typing_call_path(&call_path, "TypeVarTuple")
                {
                    Some(VarKind::TypeVarTuple)
                } else {
                    None
                }
            })
        else {
            return;
        };
        checker
            .diagnostics
            .push(Diagnostic::new(UnprefixedTypeParam { kind }, value.range()));
    }
}
