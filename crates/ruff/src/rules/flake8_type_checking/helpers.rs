use num_traits::Zero;
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, ExecutionContext};
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
