use rustpython_parser::ast::{self, Arguments, Expr, Stmt, StmtKind};

use ruff_python_ast::cast;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::definition::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;

pub(super) fn match_function_def(stmt: &Stmt) -> (&str, &Arguments, Option<&Expr>, &Vec<Stmt>) {
    match &stmt.node {
        StmtKind::FunctionDef(ast::StmtFunctionDef {
            name,
            args,
            returns,
            body,
            ..
        })
        | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            name,
            args,
            returns,
            body,
            ..
        }) => (name, args, returns.as_ref().map(|expr| &**expr), body),
        _ => panic!("Found non-FunctionDef in match_name"),
    }
}

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name(checker: &Checker, definition: &Definition) -> Option<String> {
    if let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = definition
    {
        if visibility::is_overload(&checker.ctx, cast::decorator_list(stmt)) {
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
    checker: &Checker,
    definition: &Definition,
    overloaded_name: &str,
) -> bool {
    if let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = definition
    {
        if visibility::is_overload(&checker.ctx, cast::decorator_list(stmt)) {
            false
        } else {
            let (name, ..) = match_function_def(stmt);
            name == overloaded_name
        }
    } else {
        false
    }
}
