use rustpython_parser::ast::{self, Arg, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pyupgrade::fixes;

#[violation]
pub struct SuperCallWithParameters;

impl AlwaysAutofixableViolation for SuperCallWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `super()` instead of `super(__class__, self)`")
    }

    fn autofix_title(&self) -> String {
        "Remove `__super__` parameters".to_string()
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

/// UP008
pub(crate) fn super_call_with_parameters(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `super_args` too, so this is just an optimization.)
    if !is_super_call_with_arguments(func, args) {
        return;
    }
    let scope = checker.semantic().scope();

    // Check: are we in a Function scope?
    if !scope.kind.is_any_function() {
        return;
    }

    let mut parents = checker.semantic().parents();

    // For a `super` invocation to be unnecessary, the first argument needs to match
    // the enclosing class, and the second argument needs to match the first
    // argument to the enclosing function.
    let [first_arg, second_arg] = args else {
        return;
    };

    // Find the enclosing function definition (if any).
    let Some(Stmt::FunctionDef(ast::StmtFunctionDef {
        args: parent_args, ..
    })) = parents.find(|stmt| stmt.is_function_def_stmt()) else {
        return;
    };

    // Extract the name of the first argument to the enclosing function.
    let Some(Arg {
        arg: parent_arg, ..
    }) = parent_args.args.first() else {
        return;
    };

    // Find the enclosing class definition (if any).
    let Some(Stmt::ClassDef(ast::StmtClassDef {
        name: parent_name, ..
    })) = parents.find(|stmt| stmt.is_class_def_stmt()) else {
        return;
    };

    let (
        Expr::Name(ast::ExprName {
            id: first_arg_id, ..
        }),
        Expr::Name(ast::ExprName {
            id: second_arg_id, ..
        }),
    ) = (first_arg, second_arg) else {
        return;
    };

    if !(first_arg_id == parent_name && second_arg_id == parent_arg) {
        return;
    }

    drop(parents);

    let mut diagnostic = Diagnostic::new(SuperCallWithParameters, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(edit) = fixes::remove_super_arguments(checker.locator, checker.stylist, expr) {
            diagnostic.set_fix(Fix::suggested(edit));
        }
    }
    checker.diagnostics.push(diagnostic);
}
