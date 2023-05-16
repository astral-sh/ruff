use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[violation]
pub struct BadSuperCall {
    arg: String,
    parent: String,
}

impl Violation for BadSuperCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadSuperCall { arg, parent } = self;
        format!("Bad Super Call: first argument `{arg}` does not match inherited parent `{parent}`")
    }
}
/// Returns `true` if a call is an argumented `super` invocation.
fn is_super_call_with_arguments(func: &Expr, args: &[Expr]) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        id == "super" && !args.is_empty()
    } else {
        false
    }
}

/// PLE0241
/// pub(crate) fn bad_super_call(checker: &mut Checker, name: &str, bases: &[Expr]) {
pub(crate) fn bad_super_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    // Only bother going through the super check at all if we're in a `super` call.
    if !is_super_call_with_arguments(func, args) {
        return;
    }
    let scope = checker.ctx.scope();

    // Check: are we in a Function scope?
    if !matches!(scope.kind, ScopeKind::Function(_)) {
        return;
    }

    // Extract the name of the first argument to the enclosing function (if any).
    let Some(Expr::Name(ast::ExprName {
        id: first_arg_id, ..
    })) = args.first() else {
        return;
    };

    
    // Find the parent and base class (if any).
    let mut parents = checker.ctx.parents();
    let Some(Stmt::ClassDef(ast::StmtClassDef {
        name: parent_name,
        bases, ..
    })) = parents
        .find(|stmt| matches!(stmt, Stmt::ClassDef (_))) else {
        return;
    };
    let Some(Expr::Name(ast::ExprName {
        id: base_class, ..
    })) = &bases.first() else {
        return;
    };

    // The first argument of super() must be a inherited parent
    // ATM we only check for first inherited parent or base class
    if first_arg_id != parent_name && first_arg_id != base_class {
        checker.diagnostics.push(Diagnostic::new(
            BadSuperCall{
                arg: first_arg_id.to_string(),
                parent: parent_name.to_string(),
            },
            expr.range(),
        ));
    }
}
