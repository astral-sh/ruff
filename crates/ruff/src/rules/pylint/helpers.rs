use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

pub(crate) fn in_dunder_init(checker: &Checker) -> bool {
    let scope = checker.ctx.scope();
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
    let Some(parent) = scope.parent.map(|scope_id| &checker.ctx.scopes[scope_id]) else {
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
