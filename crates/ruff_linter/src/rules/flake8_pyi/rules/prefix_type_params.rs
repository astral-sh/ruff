use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
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
/// Checks that type `TypeVar`s, `ParamSpec`s, and `TypeVarTuple`s in stubs
/// have names prefixed with `_`.
///
/// ## Why is this bad?
/// Prefixing type parameters with `_` avoids accidentally exposing names
/// internal to the stub.
///
/// ## Example
/// ```pyi
/// from typing import TypeVar
///
/// T = TypeVar("T")
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import TypeVar
///
/// _T = TypeVar("_T")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnprefixedTypeParam {
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
pub(crate) fn prefix_type_params(checker: &Checker, value: &Expr, targets: &[Expr]) {
    // If the typing modules were never imported, we'll never match below.
    if !checker.semantic().seen_typing() {
        return;
    }

    let [target] = targets else {
        return;
    };

    if let Expr::Name(ast::ExprName { id, .. }) = target {
        if id.starts_with('_') {
            return;
        }
    }

    let Expr::Call(ast::ExprCall { func, .. }) = value else {
        return;
    };

    let Some(kind) = checker
        .semantic()
        .resolve_qualified_name(func)
        .and_then(|qualified_name| {
            if checker
                .semantic()
                .match_typing_qualified_name(&qualified_name, "ParamSpec")
            {
                Some(VarKind::ParamSpec)
            } else if checker
                .semantic()
                .match_typing_qualified_name(&qualified_name, "TypeVar")
            {
                Some(VarKind::TypeVar)
            } else if checker
                .semantic()
                .match_typing_qualified_name(&qualified_name, "TypeVarTuple")
            {
                Some(VarKind::TypeVarTuple)
            } else {
                None
            }
        })
    else {
        return;
    };

    checker.report_diagnostic(Diagnostic::new(UnprefixedTypeParam { kind }, value.range()));
}
