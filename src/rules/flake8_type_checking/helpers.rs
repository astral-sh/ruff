use rustpython_ast::{Expr, Stmt};

use crate::ast::types::{Binding, BindingKind, Range};
use crate::checkers::ast::Checker;

pub fn is_type_checking_block(checker: &Checker, test: &Expr) -> bool {
    checker.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    })
}

pub fn is_valid_runtime_import(binding: &Binding, blocks: &[&Stmt]) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Importation(..)
            | BindingKind::FromImportation(..)
            | BindingKind::SubmoduleImportation(..)
    ) {
        if binding.runtime_usage.is_some() {
            return !blocks
                .iter()
                .any(|block| Range::from_located(block).contains(&binding.range));
        }
    }
    false
}
