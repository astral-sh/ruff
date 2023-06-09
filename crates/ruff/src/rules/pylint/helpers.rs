use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;
use rustpython_parser::ast;
use rustpython_parser::ast::Cmpop;
use std::fmt;

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

/// A wrapper around [`Cmpop`] that implements `Display`.
#[derive(Debug)]
pub(crate) struct CmpopExt(Cmpop);

impl From<&Cmpop> for CmpopExt {
    fn from(cmpop: &Cmpop) -> Self {
        CmpopExt(*cmpop)
    }
}

impl fmt::Display for CmpopExt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self.0 {
            Cmpop::Eq => "==",
            Cmpop::NotEq => "!=",
            Cmpop::Lt => "<",
            Cmpop::LtE => "<=",
            Cmpop::Gt => ">",
            Cmpop::GtE => ">=",
            Cmpop::Is => "is",
            Cmpop::IsNot => "is not",
            Cmpop::In => "in",
            Cmpop::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}
