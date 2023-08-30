use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_python_semantic::Binding;

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

/// PERF403
pub(crate) fn manual_dict_comprehension(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    // For a dictionary comprehension to be appropriate, the loop needs both an index
    // and a value, so the target must be a tuple.
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = target else {
        return;
    };

    let [Expr::Name(ast::ExprName { id: target_key, .. }), Expr::Name(ast::ExprName {
        id: target_value, ..
    })] = elts.as_slice()
    else {
        return;
    };

    let stmt = match body {
        // ```python
        // for idx, name in enumerate(names):
        //     if idx % 2 == 0:
        //         result[name] = idx
        // ```
        [Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        })] => {
            // TODO(charlie): If there's an `else` clause, verify that the `else` has the
            // same structure.
            if !elif_else_clauses.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            stmt
        }
        // ```python
        // for idx, name in enumerate(names):
        //     result[name] = idx
        // ```
        [stmt] => stmt,
        _ => return,
    };

    let Stmt::Assign(ast::StmtAssign {
        targets,
        value,
        range,
    }) = stmt
    else {
        return;
    };

    let [Expr::Subscript(ast::ExprSubscript {
        value: subscript_value,
        slice,
        ..
    })] = targets.as_slice()
    else {
        return;
    };

    let Expr::Name(ast::ExprName { id: key, .. }) = slice.as_ref() else {
        return;
    };

    let Expr::Name(ast::ExprName { id: value, .. }) = value.as_ref() else {
        return;
    };

    if key != target_key || value != target_value {
        return;
    }

    // Exclude non-dictionary value.
    let Expr::Name(ast::ExprName {
        id: subscript_name, ..
    }) = subscript_value.as_ref()
    else {
        return;
    };
    let scope = checker.semantic().current_scope();
    let bindings: Vec<&Binding> = scope
        .get_all(subscript_name)
        .map(|binding_id| checker.semantic().binding(binding_id))
        .collect();

    let [binding] = bindings.as_slice() else {
        return;
    };

    if !is_dict(binding, checker.semantic()) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(ManualDictComprehension, *range));
}
