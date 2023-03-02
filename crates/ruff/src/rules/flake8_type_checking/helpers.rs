use num_traits::Zero;
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::helpers::{map_callable, to_call_path};
use crate::ast::types::{Binding, BindingKind, ExecutionContext, Scope, ScopeKind};
use crate::checkers::ast::Checker;

/// Return `true` if [`Expr`] is a guard for a type-checking block.
pub fn is_type_checking_block(checker: &Checker, test: &Expr) -> bool {
    // Ex) `if False:`
    if matches!(
        test.node,
        ExprKind::Constant {
            value: Constant::Bool(false),
            ..
        }
    ) {
        return true;
    }

    // Ex) `if 0:`
    if let ExprKind::Constant {
        value: Constant::Int(value),
        ..
    } = &test.node
    {
        if value.is_zero() {
            return true;
        }
    }

    // Ex) `if typing.TYPE_CHECKING:`
    if checker.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    }) {
        return true;
    }

    false
}

pub const fn is_valid_runtime_import(binding: &Binding) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Importation(..)
            | BindingKind::FromImportation(..)
            | BindingKind::SubmoduleImportation(..)
    ) {
        binding.runtime_usage.is_some() && matches!(binding.context, ExecutionContext::Runtime)
    } else {
        false
    }
}

pub fn runtime_evaluated(checker: &Checker, scope: &Scope) -> bool {
    runtime_evaluated_due_to_baseclass(checker, scope)
        || runtime_evaluated_due_to_decorator(checker, scope)
}
pub fn runtime_evaluated_due_to_baseclass(checker: &Checker, scope: &Scope) -> bool {
    if let ScopeKind::Class(def) = &scope.kind {
        for base_class in def.bases.iter() {
            if let Some(call_path) = checker.resolve_call_path(map_callable(base_class)) {
                if checker
                    .settings
                    .flake8_type_checking
                    .runtime_evaluated_baseclasses
                    .iter()
                    .any(|base_class| to_call_path(base_class) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}
pub fn runtime_evaluated_due_to_decorator(checker: &Checker, scope: &Scope) -> bool {
    if let ScopeKind::Class(def) = &scope.kind {
        for decorator in def.decorator_list.iter() {
            if let Some(call_path) = checker.resolve_call_path(map_callable(decorator)) {
                if checker
                    .settings
                    .flake8_type_checking
                    .runtime_evaluated_decorators
                    .iter()
                    .any(|decorator| to_call_path(decorator) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}
