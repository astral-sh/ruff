use crate::ast::function_type;
use crate::ast::function_type::FunctionType;
use crate::{
    ast::types::{FunctionDef, ScopeKind},
    checkers::ast::Checker,
};

pub fn in_dunder_init(checker: &mut Checker) -> bool {
    let scope = checker.current_scope();
    let ScopeKind::Function(FunctionDef {
        name,
        decorator_list,
        ..
    }) = &scope.kind else {
        return false;
    };
    if *name != "__init__" {
        return false;
    }
    let Some(parent) = checker.current_scope_parent() else {
        return false;
    };

    if !matches!(
        function_type::classify(
            checker,
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
