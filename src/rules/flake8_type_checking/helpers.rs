use rustpython_ast::Expr;

use crate::ast::types::{Binding, BindingKind, ExecutionContext};
use crate::checkers::ast::Checker;

pub fn is_type_checking_block(checker: &Checker, test: &Expr) -> bool {
    checker.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    })
}

pub fn is_valid_runtime_import(binding: &Binding) -> bool {
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
