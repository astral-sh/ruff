use ruff_python_ast::function_type;
use ruff_python_ast::function_type::FunctionType;
use ruff_python_ast::types::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

pub fn in_dunder_init(checker: &Checker) -> bool {
    let scope = checker.ctx.current_scope();
    let ScopeKind::Function(FunctionDef {
        name,
        decorator_list,
        ..
    }): ScopeKind = scope.kind else {
        return false;
    };
    if name != "__init__" {
        return false;
    }
    let Some(parent) = checker.ctx.current_scope_parent() else {
        return false;
    };

    if !matches!(
        function_type::classify(
            &checker.ctx,
            parent,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        FunctionType::Method
    ) {
        return false;
    }
    true
}
