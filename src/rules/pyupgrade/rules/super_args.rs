use crate::ast::helpers;
use crate::ast::types::{Range, Scope, ScopeKind};

use crate::registry::Diagnostic;

use crate::rules::pyupgrade::rules::SuperCallWithParameters;

use rustpython_ast::{ArgData, Expr, ExprKind, Stmt, StmtKind};

/// UP008
pub fn super_args(
    scope: &Scope,
    parents: &[&Stmt],
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Diagnostic> {
    if !helpers::is_super_call_with_arguments(func, args) {
        return None;
    }

    // Check: are we in a Function scope?
    if !matches!(scope.kind, ScopeKind::Function { .. }) {
        return None;
    }

    let mut parents = parents.iter().rev();

    // For a `super` invocation to be unnecessary, the first argument needs to match
    // the enclosing class, and the second argument needs to match the first
    // argument to the enclosing function.
    let [first_arg, second_arg] = args else {
        return None;
    };

    // Find the enclosing function definition (if any).
    let Some(StmtKind::FunctionDef {
        args: parent_args, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::FunctionDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    // Extract the name of the first argument to the enclosing function.
    let Some(ArgData {
        arg: parent_arg, ..
    }) = parent_args.args.first().map(|expr| &expr.node) else {
        return None;
    };

    // Find the enclosing class definition (if any).
    let Some(StmtKind::ClassDef {
        name: parent_name, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::ClassDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    let (
        ExprKind::Name {
            id: first_arg_id, ..
        },
        ExprKind::Name {
            id: second_arg_id, ..
        },
    ) = (&first_arg.node, &second_arg.node) else {
        return None;
    };

    if first_arg_id == parent_name && second_arg_id == parent_arg {
        return Some(Diagnostic::new(
            SuperCallWithParameters,
            Range::from_located(expr),
        ));
    }

    None
}
