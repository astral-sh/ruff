use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_list;

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
#[derive(ViolationMetadata)]
pub(crate) struct ManualListCopy;

impl Violation for ManualListCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `list` or `list.copy` to create a copy of a list".to_string()
    }
}

/// PERF402
pub(crate) fn manual_list_copy(checker: &Checker, for_stmt: &ast::StmtFor) {
    if for_stmt.is_async {
        return;
    }

    let Expr::Name(ast::ExprName { id, .. }) = &*for_stmt.target else {
        return;
    };

    let [stmt] = &*for_stmt.body else {
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

    let [arg] = &**args else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    if !matches!(attr.as_str(), "append" | "insert") {
        return;
    }

    // Only flag direct list copies (e.g., `for x in y: filtered.append(x)`).
    if arg.as_name_expr().is_none_or(|arg| arg.id != *id) {
        return;
    }

    // Avoid, e.g., `for x in y: filtered[x].append(x)`.
    if any_over_expr(value, &|expr| {
        expr.as_name_expr().is_some_and(|expr| expr.id == *id)
    }) {
        return;
    }

    // Avoid non-list values.
    let Some(name) = value.as_name_expr() else {
        return;
    };
    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_list(binding, checker.semantic()) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(ManualListCopy, *range));
}
