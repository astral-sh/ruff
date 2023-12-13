use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_semantic::analyze::typing::find_assigned_value;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for explicit casts to `list` on for-loop iterables.
///
/// ## Why is this bad?
/// Using a `list()` call to eagerly iterate over an already-iterable type
/// (like a tuple, list, or set) is inefficient, as it forces Python to create
/// a new list unnecessarily.
///
/// Removing the `list()` call will not change the behavior of the code, but
/// may improve performance.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// items = (1, 2, 3)
/// for i in list(items):
///     print(i)
/// ```
///
/// Use instead:
/// ```python
/// items = (1, 2, 3)
/// for i in items:
///     print(i)
/// ```
#[violation]
pub struct UnnecessaryListCast;

impl AlwaysFixableViolation for UnnecessaryListCast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not cast an iterable to `list` before iterating over it")
    }

    fn fix_title(&self) -> String {
        format!("Remove `list()` cast")
    }
}

/// PERF101
pub(crate) fn unnecessary_list_cast(checker: &mut Checker, iter: &Expr, body: &[Stmt]) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords: _,
                range: _,
            },
        range: list_range,
    }) = iter
    else {
        return;
    };

    let [arg] = args.as_slice() else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };

    if !(id == "list" && checker.semantic().is_builtin("list")) {
        return;
    }

    match arg {
        Expr::Tuple(ast::ExprTuple {
            range: iterable_range,
            ..
        })
        | Expr::List(ast::ExprList {
            range: iterable_range,
            ..
        })
        | Expr::Set(ast::ExprSet {
            range: iterable_range,
            ..
        }) => {
            let mut diagnostic = Diagnostic::new(UnnecessaryListCast, *list_range);
            diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
            checker.diagnostics.push(diagnostic);
        }
        Expr::Name(ast::ExprName {
            id,
            range: iterable_range,
            ..
        }) => {
            // If the variable is being appended to, don't suggest removing the cast:
            //
            // ```python
            // items = ["foo", "bar"]
            // for item in list(items):
            //    items.append("baz")
            // ```
            //
            // Here, removing the `list()` cast would change the behavior of the code.
            if body.iter().any(|stmt| match_append(stmt, id)) {
                return;
            }
            let Some(value) = find_assigned_value(id, checker.semantic()) else {
                return;
            };
            if matches!(value, Expr::Tuple(_) | Expr::List(_) | Expr::Set(_)) {
                let mut diagnostic = Diagnostic::new(UnnecessaryListCast, *list_range);
                diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => {}
    }
}

/// Check if a statement is an `append` call to a given identifier.
///
/// For example, `foo.append(bar)` would return `true` if `id` is `foo`.
fn match_append(stmt: &Stmt, id: &str) -> bool {
    let Some(ast::StmtExpr { value, .. }) = stmt.as_expr_stmt() else {
        return false;
    };
    let Some(ast::ExprCall { func, .. }) = value.as_call_expr() else {
        return false;
    };
    let Some(ast::ExprAttribute { value, attr, .. }) = func.as_attribute_expr() else {
        return false;
    };
    if attr != "append" {
        return false;
    }
    let Some(ast::ExprName { id: target_id, .. }) = value.as_name_expr() else {
        return false;
    };
    target_id == id
}

/// Generate a [`Fix`] to remove a `list` cast from an expression.
fn remove_cast(list_range: TextRange, iterable_range: TextRange) -> Fix {
    Fix::safe_edits(
        Edit::deletion(list_range.start(), iterable_range.start()),
        [Edit::deletion(iterable_range.end(), list_range.end())],
    )
}
