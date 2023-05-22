use rustpython_parser::ast::{self, Arguments, Expr, Stmt};

use ruff_python_ast::cast;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::definition::{Definition, Member, MemberKind};
use ruff_python_semantic::model::SemanticModel;

pub(super) fn match_function_def(
    stmt: &Stmt,
) -> (&str, &Arguments, Option<&Expr>, &[Stmt], &[Expr]) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef {
            name,
            args,
            returns,
            body,
            decorator_list,
            ..
        })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            name,
            args,
            returns,
            body,
            decorator_list,
            ..
        }) => (
            name,
            args,
            returns.as_ref().map(|expr| &**expr),
            body,
            decorator_list,
        ),
        _ => panic!("Found non-FunctionDef in match_name"),
    }
}

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name(model: &SemanticModel, definition: &Definition) -> Option<String> {
    if let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = definition
    {
        if visibility::is_overload(model, cast::decorator_list(stmt)) {
            let (name, ..) = match_function_def(stmt);
            Some(name.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Return `true` if the definition is the implementation for an overloaded
/// function.
pub(crate) fn is_overload_impl(
    model: &SemanticModel,
    definition: &Definition,
    overloaded_name: &str,
) -> bool {
    if let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = definition
    {
        if visibility::is_overload(model, cast::decorator_list(stmt)) {
            false
        } else {
            let (name, ..) = match_function_def(stmt);
            name == overloaded_name
        }
    } else {
        false
    }
}
