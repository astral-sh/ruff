use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{ExprKind, Stmt, StmtKind};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Empty,
}

/// ## What it does
/// Checks for assigning result of a function call, where the function returns None
/// Used when an assignment is done on a function call but the inferred function returns nothing but None argument.
///
/// ## Why is this bad?
/// This unnecessarily abstracts a potential bug by "hard-coding" a return of None
///
/// ## Example
/// ```python
/// def func():
///     return None
///
/// def foo():
///     return func()
/// ```
#[violation]
pub struct AssignmentFromNone {
    kind: Kind,
}

impl Violation for AssignmentFromNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssignmentFromNone { kind } = self;
        match kind {
            Kind::Empty => format!("Return statement found"),
        }
    }
}
/// PLE1128
pub fn assignment_from_none(checker: &mut Checker, body: &Stmt) {
    // if return statement with a value
    if let StmtKind::Return { value: Some(expr) } = body.node() {
        // if function call in that return statement
        if let ExprKind::Call { func, .. } = expr.node() {
            // need assistance on how to use Visitor here
            // let function_name = func.node.name();
            // for node in ast.iter() {
            //     match node.kind {
            //         StmtKind::FunctionDef { name, returns, .. } if name == function_name => {
            //              You've found the function definition
            //              Now you can inspect its body to see if it returns None
            //         }
            //         _ => {}
            //     }
            // }
            checker.diagnostics.push(Diagnostic::new(
                AssignmentFromNone { kind: Kind::Empty },
                func.range(),
            ));
        }
    }
}
