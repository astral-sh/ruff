use std::fmt;

use rustpython_parser::ast;
use rustpython_parser::ast::{CmpOp, Constant, Expr, Keyword};

use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::settings::Settings;

/// Returns the value of the `name` parameter to, e.g., a `TypeVar` constructor.
pub(super) fn type_param_name<'a>(args: &'a [Expr], keywords: &'a [Keyword]) -> Option<&'a str> {
    // Handle both `TypeVar("T")` and `TypeVar(name="T")`.
    let name_param = keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .map_or(false, |keyword| keyword.as_str() == "name")
        })
        .map(|keyword| &keyword.value)
        .or_else(|| args.get(0))?;
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(name),
        ..
    }) = &name_param
    {
        Some(name)
    } else {
        None
    }
}

pub(super) fn in_dunder_init(semantic: &SemanticModel, settings: &Settings) -> bool {
    let scope = semantic.scope();
    let (ScopeKind::Function(ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    })
    | ScopeKind::AsyncFunction(ast::StmtAsyncFunctionDef {
        name,
        decorator_list,
        ..
    })) = scope.kind
    else {
        return false;
    };
    if name != "__init__" {
        return false;
    }
    let Some(parent) = scope.parent.map(|scope_id| &semantic.scopes[scope_id]) else {
        return false;
    };

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            semantic,
            &settings.pep8_naming.classmethod_decorators,
            &settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return false;
    }
    true
}

/// A wrapper around [`CmpOp`] that implements `Display`.
#[derive(Debug)]
pub(super) struct CmpOpExt(CmpOp);

impl From<&CmpOp> for CmpOpExt {
    fn from(cmp_op: &CmpOp) -> Self {
        CmpOpExt(*cmp_op)
    }
}

impl fmt::Display for CmpOpExt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self.0 {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}
