use rustpython_parser::ast::{Arguments, Expr, Stmt, StmtKind};

use crate::ast::cast;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};
use crate::visibility;

pub(super) fn match_function_def(
    stmt: &Stmt,
) -> (&str, &Arguments, &Option<Box<Expr>>, &Vec<Stmt>) {
    match &stmt.node {
        StmtKind::FunctionDef {
            name,
            args,
            returns,
            body,
            ..
        }
        | StmtKind::AsyncFunctionDef {
            name,
            args,
            returns,
            body,
            ..
        } => (name, args, returns, body),
        _ => panic!("Found non-FunctionDef in match_name"),
    }
}

/// Return the name of the function, if it's overloaded.
pub fn overloaded_name(checker: &Checker, definition: &Definition) -> Option<String> {
    if let DefinitionKind::Function(stmt)
    | DefinitionKind::NestedFunction(stmt)
    | DefinitionKind::Method(stmt) = definition.kind
    {
        if visibility::is_overload(checker, cast::decorator_list(stmt)) {
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
pub fn is_overload_impl(checker: &Checker, definition: &Definition, overloaded_name: &str) -> bool {
    if let DefinitionKind::Function(stmt)
    | DefinitionKind::NestedFunction(stmt)
    | DefinitionKind::Method(stmt) = definition.kind
    {
        if visibility::is_overload(checker, cast::decorator_list(stmt)) {
            false
        } else {
            let (name, ..) = match_function_def(stmt);
            name == overloaded_name
        }
    } else {
        false
    }
}
