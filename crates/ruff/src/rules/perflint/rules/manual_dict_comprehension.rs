use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a dictionary comprehension.
///
/// ## Why is this bad?
/// When creating a dictionary in a for-loop, prefer a dictionary
/// comprehension. Comprehensions are more readable and more performant.
///
/// Using the below as an example, the dictionary comprehension is ~10% faster
/// on Python 3.11, and ~25% faster on Python 3.10.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
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
pub struct ManualDictComprehension;

impl Violation for ManualDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a dictionary comprehension instead of a for-loop")
    }
}

/// PERF404
pub(crate) fn manual_dict_comprehension(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let [stmt] = body else {
        return;
    };

    // For a dictionary comprehension to be appropriate, the loop needs both an index
    // and a value, so the target must be a tuple.
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = target else {
        return;
    };

    let names = elts
        .iter()
        .filter_map(|elt| {
            if let Expr::Name(ast::ExprName { id, .. }) = elt {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if names.is_empty() {
        return;
    }

    if let Stmt::If(ast::StmtIf { body: if_body, .. }) = stmt {
        // Key-value assignment within an if-statement (e.g., `if condition: result[key] = value`).
        for stmt in if_body {
            check_for_slow_dict_creation(checker, &names, stmt);
        }
    } else {
        // Direct key-value assignment (e.g., `result[key] = value`).
        check_for_slow_dict_creation(checker, &names, stmt);
    }
}

fn check_for_slow_dict_creation(checker: &mut Checker, names: &[&str], stmt: &Stmt) {
    let Stmt::Assign(ast::StmtAssign {
        targets,
        value,
        range,
        ..
    }) = stmt
    else {
        return;
    };

    let [Expr::Subscript(ast::ExprSubscript { slice, .. })] = targets.as_slice() else {
        return;
    };

    let Expr::Name(ast::ExprName { id: key, .. }) = slice.as_ref() else {
        return;
    };

    let Expr::Name(ast::ExprName { id: value, .. }) = value.as_ref() else {
        return;
    };

    if names.contains(&value.as_str()) && names.contains(&key.as_str()) {
        checker
            .diagnostics
            .push(Diagnostic::new(ManualDictComprehension, *range));
    }
}
