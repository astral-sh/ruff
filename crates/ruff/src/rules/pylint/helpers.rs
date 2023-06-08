use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::analyze::function_type::FunctionType;
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;
use rustpython_parser::ast;
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum ViolationsCmpop {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl From<&ast::Cmpop> for ViolationsCmpop {
    fn from(cmpop: &ast::Cmpop) -> Self {
        match cmpop {
            ast::Cmpop::Eq => Self::Eq,
            ast::Cmpop::NotEq => Self::NotEq,
            ast::Cmpop::Lt => Self::Lt,
            ast::Cmpop::LtE => Self::LtE,
            ast::Cmpop::Gt => Self::Gt,
            ast::Cmpop::GtE => Self::GtE,
            ast::Cmpop::Is => Self::Is,
            ast::Cmpop::IsNot => Self::IsNot,
            ast::Cmpop::In => Self::In,
            ast::Cmpop::NotIn => Self::NotIn,
        }
    }
}

impl fmt::Display for ViolationsCmpop {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self {
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::Lt => "<",
            Self::LtE => "<=",
            Self::Gt => ">",
            Self::GtE => ">=",
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::In => "in",
            Self::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}
