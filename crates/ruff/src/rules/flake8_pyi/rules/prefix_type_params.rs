use std::fmt;

use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum VarKind {
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
    pub kind: VarKind,
}

impl Violation for UnprefixedTypeParam {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnprefixedTypeParam { kind } = self;
        format!("Name of private `{kind}` must start with `_`")
    }
}

/// PYI001
pub fn prefix_type_params(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    if targets.len() != 1 {
        return;
    }
    if let ExprKind::Name { id, .. } = &targets[0].node {
        if id.starts_with('_') {
            return;
        }
    };

    if let ExprKind::Call { func, .. } = &value.node {
        let Some(kind) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
            if checker.ctx.match_typing_call_path(&call_path, "ParamSpec") {
                Some(VarKind::ParamSpec)
            } else if checker.ctx.match_typing_call_path(&call_path, "TypeVar") {
                Some(VarKind::TypeVar)
            } else if checker.ctx.match_typing_call_path(&call_path, "TypeVarTuple") {
                Some(VarKind::TypeVarTuple)
            } else {
                None
            }
        }) else {
            return;
        };
        checker.diagnostics.push(Diagnostic::new(
            UnprefixedTypeParam { kind },
            Range::from(value),
        ));
    }
}
