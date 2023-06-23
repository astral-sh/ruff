use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::{ast, Expr, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for cases where a new dictionary is made as a copy of an existing one by setting values
/// in a for-loop
///
/// ## Why is this bad?
/// It is more performant to use a dict comprehension to construct dictionaries
///
/// ## Example
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {}
/// for x, y in pairs:
///     if y % 2:
///         result[x] = y
/// ```
///
/// Use instead:
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {x: y for x, y in pairs if y % 2}
/// ```
#[violation]
pub struct SlowDictCreation;

impl Violation for SlowDictCreation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a dict comprehension to create a (filtered) copy of a dict")
    }
}

/// PERF404
pub(crate) fn slow_dict_creation(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let [stmt] = body else {
		return;
	};

    // For a dict comprehension to make sense the for loop must have both an index and a value
    // so the target must be a tuple
    let Expr::Tuple(ast::ExprTuple{elts, ..}) = target else {
        return
    };

    let mut names = Vec::new();
    for elt in elts.iter() {
        let Expr::Name(ast::ExprName { id, .. }) = elt else {
            continue
        };
        names.push(id);
    }

    if names.is_empty() {
        return;
    }

    // Check 1: Dict assignment
    check_for_slow_dict_creation(checker, names.as_slice(), stmt);

    // Check 2: Dict assignment in an if statement
    if let Stmt::If(ast::StmtIf { body: if_body, .. }) = stmt {
        for stmt in if_body {
            check_for_slow_dict_creation(checker, names.as_slice(), stmt);
        }
    };
}

fn check_for_slow_dict_creation(checker: &mut Checker, names: &[&String], stmt: &Stmt) {
    let Stmt::Assign(ast::StmtAssign { targets, value, range, .. })= stmt else {
        return;
    };

    let [Expr::Subscript(ast::ExprSubscript { slice, .. })] = targets.as_slice() else {
            return
     };

    let Expr::Name(ast::ExprName { id: id_index, .. }) = slice.as_ref() else {
        return
    };

    let Expr::Name(ast::ExprName { id: id_value, .. }) = value.as_ref() else {
        return
    };

    if names.contains(&id_value) && names.contains(&id_index) {
        checker
            .diagnostics
            .push(Diagnostic::new(SlowDictCreation, *range));
    }
}
