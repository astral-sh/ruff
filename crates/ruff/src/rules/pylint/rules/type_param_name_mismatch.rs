use std::fmt;

use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum VarKind {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
    NewType,
}

impl fmt::Display for VarKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarKind::TypeVar => fmt.write_str("TypeVar"),
            VarKind::ParamSpec => fmt.write_str("ParamSpec"),
            VarKind::TypeVarTuple => fmt.write_str("TypeVarTuple"),
            VarKind::NewType => fmt.write_str("NewType"),
        }
    }
}

#[violation]
pub struct TypeParamNameMismatch {
    kind: VarKind,
    var_name: String,
    param_name: String,
}

impl Violation for TypeParamNameMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeParamNameMismatch {
            kind,
            var_name,
            param_name,
        } = self;
        format!("`{kind}` name `{param_name}` does not match assigned variable name `{var_name}`")
    }
}

fn param_name(value: &Expr) -> Option<String> {
    if let Expr::Call(ast::ExprCall { args, keywords, .. }) = value {
        let name_param = args.get(0);
        if name_param.is_none() && !keywords.is_empty() {
            if let Some(keyword) = find_keyword(keywords, "name") {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(name_param_value),
                    ..
                }) = &keyword.value
                {
                    return Some(name_param_value.to_string());
                }
            }
        } else if let Some(Expr::Constant(ast::ExprConstant {
            value: Constant::Str(name_param_value),
            ..
        })) = name_param
        {
            return Some(name_param_value.to_string());
        }
    };
    None
}

/// PLC0132
pub(crate) fn type_param_name_mismatch(checker: &mut Checker, value: &Expr, targets: &[Expr]) {
    if targets.len() != 1 {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = &targets[0] else { return; };
    if let Some(param_name) = param_name(value) {
        if id == &param_name {
            return;
        }
        if let Expr::Call(ast::ExprCall { func, .. }) = value {
            let Some(kind) = checker
                .semantic()
                .resolve_call_path(func)
                .and_then(|call_path| {
                    if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "ParamSpec")
                    {
                        Some(VarKind::ParamSpec)
                    } else if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "TypeVar")
                    {
                        Some(VarKind::TypeVar)
                    } else if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "TypeVarTuple")
                    {
                        Some(VarKind::TypeVarTuple)
                    } else if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "NewType")
                    {
                        Some(VarKind::NewType)
                    } else {
                        None
                    }
                })
                else {
                    return;
                };
            checker.diagnostics.push(Diagnostic::new(
                TypeParamNameMismatch {
                    kind,
                    var_name: id.to_string(),
                    param_name,
                },
                value.range(),
            ));
        }
    };
}
