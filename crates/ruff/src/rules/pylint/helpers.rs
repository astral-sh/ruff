use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;
use rustpython_parser::ast;

use crate::settings::Settings;

pub(crate) fn in_dunder_init(model: &SemanticModel, settings: &Settings) -> bool {
    let scope = model.scope();
    let (
        ScopeKind::Function(ast::StmtFunctionDef {
            name,
            decorator_list,
        ..
        }) |
        ScopeKind::AsyncFunction(ast::StmtAsyncFunctionDef {
            name,
            decorator_list,
            ..
        })
    ) = scope.kind else {
        return false;
    };
    if name != "__init__" {
        return false;
    }
    let Some(parent) = scope.parent.map(|scope_id| &model.scopes[scope_id]) else {
        return false;
    };

    if !matches!(
        function_type::classify(
            model,
            parent,
            name,
            decorator_list,
            &settings.pep8_naming.classmethod_decorators,
            &settings.pep8_naming.staticmethod_decorators,
        ),
        FunctionType::Method
    ) {
        return false;
    }
    true
}
