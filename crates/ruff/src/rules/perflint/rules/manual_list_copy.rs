use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::Binding;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a making a copy of a list.
///
/// ## Why is this bad?
/// When creating a copy of an existing list using a for-loop, prefer
/// `list` or `list.copy` instead. Making a direct copy is more readable and
/// more performant.
///
/// Using the below as an example, the `list`-based copy is ~2x faster on
/// Python 3.11.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// original = list(range(10000))
/// filtered = []
/// for i in original:
///     filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = list(range(10000))
/// filtered = list(original)
/// ```
#[violation]
pub struct ManualListCopy;

impl Violation for ManualListCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `list` or `list.copy` to create a copy of a list")
    }
}

/// PERF402
pub(crate) fn manual_list_copy(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    let [stmt] = body else {
        return;
    };

    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords,
                range: _,
            },
        range,
    }) = value.as_ref()
    else {
        return;
    };

    if !keywords.is_empty() {
        return;
    }

    let [arg] = args.as_slice() else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    if !matches!(attr.as_str(), "append" | "insert") {
        return;
    }

    // Only flag direct list copies (e.g., `for x in y: filtered.append(x)`).
    if !arg.as_name_expr().is_some_and(|arg| arg.id == *id) {
        return;
    }

    // Avoid, e.g., `for x in y: filtered[x].append(x)`.
    if any_over_expr(value, &|expr| {
        expr.as_name_expr().is_some_and(|expr| expr.id == *id)
    }) {
        return;
    }

    // Avoid non-list values.
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    let bindings: Vec<&Binding> = checker
        .semantic()
        .current_scope()
        .get_all(id)
        .map(|binding_id| checker.semantic().binding(binding_id))
        .collect();

    let [binding] = bindings.as_slice() else {
        return;
    };

    if !is_list(binding, checker.semantic()) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(ManualListCopy, *range));
}
